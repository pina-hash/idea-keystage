//! Holding input while the password check of a new focus is pending
//! (ARCHITECTURE.md section 8, rule 3; DECISIONS item 6, "fail closed").
//!
//! UI Automation answers `IsPassword` for a newly focused browser, WPF, UWP
//! or Electron field tens of milliseconds after the focus moved, and the
//! low-level hook sees every key before the target app has even handled
//! the Tab that moved the focus. Keys typed in that window must never reach
//! the processor as visible input, and suppressing the whole app on every
//! focus change would flash the "hidden" indicator and break chords and
//! repeat counts. So the platform layer holds them back instead:
//!
//! * The watcher thread is the only writer of a [`PasswordHold`] word: a
//!   hold *epoch* plus a [`HoldMode`]. It starts a new epoch on every focus
//!   or foreground change and marks it as a *focus epoch* (`Hold` while UI
//!   Automation classifies the new focus; `Open` or `Drop` when only the
//!   Win32 check runs). It ends a `Hold` epoch when UI Automation has
//!   classified the new focus: [`Outcome::Release`] for "not a password",
//!   otherwise [`Outcome::Drop`] or [`Outcome::Discard`] (below).
//! * The hook procedures copy the current word into every record (one
//!   atomic load), so each record carries the state the watcher knew when
//!   the key was pressed, in the order of the hook stream.
//! * The input receiver ([`HoldFilter`]) hands `Open` records out, keeps
//!   `Hold` records, in order, until their epoch is resolved, and drops the
//!   rest. Dropped keys stay dropped until released: a key whose press was
//!   dropped never has its release (or auto-repeat) handed out, which the
//!   processor would otherwise treat as a fresh press.
//! * How a record is dropped decides what happens to modifier keys and
//!   Caps Lock, which type nothing but light keys, play sounds and make
//!   modifier-tap callouts when the processor sees them unsuppressed:
//!   * [`HoldMode::Drop`] records and [`Outcome::Drop`]: the watcher told
//!     the app `Password { active: true }` *before* it published the word,
//!     so the processor is suppressed when these records reach it.
//!     Modifiers and Caps Lock pass, for its modifier and Caps Lock state
//!     only; nothing about them is shown.
//!   * [`Outcome::Discard`]: the epoch ended while the app was not told to
//!     suppress (a newer focus change before the verdict, UI Automation
//!     lost, too much held, the watcher stopped). Everything is dropped,
//!     modifiers and Caps Lock too. A modifier whose press was handed out
//!     earlier is released by an `InputEvent::ModifierResync`, which never
//!     makes a tap callout. A dropped Caps Lock toggle is reported by
//!     [`HoldFilter::take_caps_lock_unsynced`].
//! * Provisional hold. The watcher hears of a focus move only after the
//!   target app made it (an out-of-context WinEvent), while the hook sees
//!   the keys typed after a Tab or a click before the app has even handled
//!   that Tab or click. Those keys carry the old, open stamp. So once the
//!   receiver hands out a focus-moving input (a Tab, Enter, Esc or F6
//!   key-down, or a mouse button-down, [`moves_focus`]) it keeps back the
//!   key records that follow until the watcher publishes a focus epoch.
//!   They are then decided by that epoch's verdict *and* their own stamp:
//!   handed out only if both say "open". With no focus epoch within
//!   [`PROVISIONAL_MS`] the input did not move the focus, and they go by
//!   their own stamp. Mouse records are not delayed by this unless a key
//!   is kept back before them (order is kept).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};

use keystage_core::keys::{self, vk, KeyClass};
use keystage_core::{InputEvent, KeyInput, Mod, ModKey, MouseButton, Side};

/// What the receiver does with records stamped in an epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HoldMode {
    /// No password field has the focus: hand records out.
    Open,
    /// The focus changed and is not classified yet: keep records until the
    /// epoch is resolved.
    Hold,
    /// A password field has the focus, or the focus could not be classified
    /// (fail closed): drop records. The app suppresses meanwhile.
    Drop,
}

/// How a `Hold` epoch ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// The new focus is not a password field: hand the held records out.
    Release,
    /// Drop them; the app was told to suppress before this was published,
    /// so modifier and Caps Lock records still pass for its bookkeeping.
    Drop,
    /// Drop them all, modifiers and Caps Lock included: the app is not
    /// suppressing (a newer focus change, UI Automation lost, stopping).
    Discard,
}

const MODE_BITS: u32 = 2;
const MODE_MASK: u32 = (1 << MODE_BITS) - 1;
const EPOCH_MASK: u32 = u32::MAX >> MODE_BITS;
/// Epochs remembered (resolutions and starts). A record older than this
/// many transitions that is still held somewhere is dropped (fail closed).
const RING: usize = 256;

/// Start records: bits 0 and 1 the mode, bit 2 "focus epoch", bit 3 "set",
/// the epoch from bit 4 (its top bits do not fit; the ring index and the
/// low bits identify it well enough within [`RING`] transitions).
const START_FOCUS: u32 = 1 << 2;
const START_SET: u32 = 1 << 3;
const START_EPOCH_SHIFT: u32 = 4;

fn mode_code(mode: HoldMode) -> u32 {
    match mode {
        HoldMode::Open => 0,
        HoldMode::Hold => 1,
        HoldMode::Drop => 2,
    }
}

fn mode_of(code: u32) -> HoldMode {
    match code & MODE_MASK {
        0 => HoldMode::Open,
        1 => HoldMode::Hold,
        // 3 is never written; anything unexpected drops.
        _ => HoldMode::Drop,
    }
}

fn encode(epoch: u32, mode: HoldMode) -> u32 {
    ((epoch & EPOCH_MASK) << MODE_BITS) | mode_code(mode)
}

/// The epoch and mode of a stamp.
pub fn decode(word: u32) -> (u32, HoldMode) {
    (word >> MODE_BITS, mode_of(word))
}

fn encode_outcome(epoch: u32, outcome: Outcome) -> u32 {
    // Codes 1 to 3, so an untouched slot (0) never matches.
    let o = match outcome {
        Outcome::Release => 1,
        Outcome::Drop => 2,
        Outcome::Discard => 3,
    };
    ((epoch & EPOCH_MASK) << MODE_BITS) | o
}

fn encode_start(epoch: u32, mode: HoldMode, focus: bool) -> u32 {
    (epoch << START_EPOCH_SHIFT) | START_SET | if focus { START_FOCUS } else { 0 } | mode_code(mode)
}

/// True when epoch `a` comes before epoch `b` (epochs wrap).
fn epoch_before(a: u32, b: u32) -> bool {
    let d = b.wrapping_sub(a) & EPOCH_MASK;
    d != 0 && d <= EPOCH_MASK / 2
}

/// What [`PasswordHold::focus_after`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusLookup {
    /// No focus epoch started after the anchor yet.
    NotYet,
    /// The first focus epoch after the anchor, as a stamp.
    Found(u32),
    /// The history no longer reaches back to the anchor.
    Lost,
}

/// The hold state shared by the watcher (the one writer), the hook
/// procedures (which stamp records with [`PasswordHold::stamp`]) and the
/// input receiver. Only atomics: safe to read inside a hook procedure.
#[derive(Debug)]
pub struct PasswordHold {
    word: AtomicU32,
    resolved: [AtomicU32; RING],
    started: [AtomicU32; RING],
}

impl Default for PasswordHold {
    fn default() -> Self {
        PasswordHold::new()
    }
}

impl PasswordHold {
    /// Epoch 0, `Open`.
    pub const fn new() -> PasswordHold {
        PasswordHold {
            word: AtomicU32::new(0),
            resolved: [const { AtomicU32::new(0) }; RING],
            started: [const { AtomicU32::new(0) }; RING],
        }
    }

    /// The word a hook procedure copies into its record: one atomic load,
    /// no other work.
    #[inline(always)]
    pub fn stamp(&self) -> u32 {
        self.word.load(Ordering::Acquire)
    }

    /// The current mode.
    pub fn mode(&self) -> HoldMode {
        decode(self.stamp()).1
    }

    /// Ends the current epoch and starts a new one in `mode`. When the
    /// current epoch is a `Hold`, `outcome` resolves it (it is ignored
    /// otherwise). `focus` marks the new epoch as started by a focus or
    /// foreground change: input kept back provisionally after a Tab or a
    /// click is decided by the first such epoch. Returns the new word.
    ///
    /// Threading: the watcher thread only (single writer). The resolution
    /// and the start record are stored before the new word is published, so
    /// a reader that sees a later epoch also sees them.
    pub fn publish(&self, mode: HoldMode, outcome: Outcome, focus: bool) -> u32 {
        let (epoch, current) = decode(self.word.load(Ordering::Acquire));
        if current == HoldMode::Hold {
            if let Some(slot) = self.resolved.get(epoch as usize % RING) {
                slot.store(encode_outcome(epoch, outcome), Ordering::Release);
            }
        }
        let next_epoch = epoch.wrapping_add(1) & EPOCH_MASK;
        if let Some(slot) = self.started.get(next_epoch as usize % RING) {
            slot.store(encode_start(next_epoch, mode, focus), Ordering::Release);
        }
        let next = encode(next_epoch, mode);
        self.word.store(next, Ordering::Release);
        next
    }

    /// How the epoch of `stamp` ended: `None` while it is still the current
    /// epoch. An epoch whose resolution is no longer remembered is
    /// discarded.
    pub fn resolution(&self, stamp: u32) -> Option<Outcome> {
        let (epoch, _) = decode(stamp);
        let (now, _) = decode(self.word.load(Ordering::Acquire));
        if now == epoch {
            return None;
        }
        let slot = self
            .resolved
            .get(epoch as usize % RING)
            .map(|s| s.load(Ordering::Acquire));
        if slot == Some(encode_outcome(epoch, Outcome::Release)) {
            Some(Outcome::Release)
        } else if slot == Some(encode_outcome(epoch, Outcome::Drop)) {
            Some(Outcome::Drop)
        } else {
            Some(Outcome::Discard)
        }
    }

    /// The first focus epoch started after epoch `anchor`.
    fn focus_after(&self, anchor: u32) -> FocusLookup {
        let (now, _) = decode(self.word.load(Ordering::Acquire));
        let span = now.wrapping_sub(anchor) & EPOCH_MASK;
        if span > EPOCH_MASK / 2 {
            // The anchor is not in the past: nothing can be trusted.
            return FocusLookup::Lost;
        }
        for i in 1..=span {
            if i as usize > RING {
                return FocusLookup::Lost;
            }
            let e = anchor.wrapping_add(i) & EPOCH_MASK;
            let entry = self
                .started
                .get(e as usize % RING)
                .map_or(0, |s| s.load(Ordering::Acquire));
            let id_mask = !((1 << START_EPOCH_SHIFT) - 1);
            if entry & START_SET == 0 || entry & id_mask != (e << START_EPOCH_SHIFT) & id_mask {
                return FocusLookup::Lost;
            }
            if entry & START_FOCUS != 0 {
                return FocusLookup::Found(encode(e, mode_of(entry)));
            }
        }
        FocusLookup::NotYet
    }
}

/// Records kept back at most; beyond this the held records are dropped
/// (fail closed). About 8 seconds of a 1000 Hz mouse.
pub const MAX_HELD: usize = 8_192;

/// How long key records typed after a focus-moving input wait for the
/// watcher to report a focus change, counted from that input. Long enough
/// for the target app to handle the input and for the out-of-context
/// WinEvent to reach the watcher on a loaded machine (both usually take a
/// few milliseconds); short enough that a Tab, Enter, Esc or click that
/// moves no focus delays the keys typed right after it only a little. It
/// is not a guarantee: an app that takes longer to move the focus than
/// this leaves the keys to their own stamp.
pub const PROVISIONAL_MS: u64 = 150;

/// F6 moves the focus between the panes of a browser or Explorer window.
const VK_F6: u16 = 0x75;

/// A dropped or handed-out press older than this whose release never came
/// (the hooks were out, say) no longer decides what happens to the key.
const STALE_MS: u64 = 2_000;
/// Keys tracked at most (keys held down at the same time).
const MAX_TRACKED: usize = 64;

/// True for an input that commonly moves the keyboard focus: a Tab, Enter,
/// Esc or F6 key-down (with or without modifiers: Shift+Tab, Alt+Tab,
/// Ctrl+Tab), and every mouse button-down.
pub fn moves_focus(ev: &InputEvent) -> bool {
    match ev {
        InputEvent::Key(k) => k.down && matches!(k.vk, vk::TAB | vk::RETURN | vk::ESCAPE | VK_F6),
        InputEvent::Button { down, .. } => *down,
        InputEvent::Wheel { .. } | InputEvent::Move { .. } | InputEvent::ModifierResync { .. } => {
            false
        }
    }
}

/// What happens to one record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    /// Hand it out.
    Pass,
    /// Drop it; the app suppresses, so modifiers and Caps Lock pass.
    Suppressed,
    /// Drop it, modifiers and Caps Lock included.
    Discard,
}

impl From<Outcome> for Decision {
    fn from(o: Outcome) -> Decision {
        match o {
            Outcome::Release => Decision::Pass,
            Outcome::Drop => Decision::Suppressed,
            Outcome::Discard => Decision::Discard,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// Its press was handed out: hand out its release too.
    Passed,
    /// Its press was dropped: drop its auto-repeats and release.
    Swallowed,
}

#[derive(Clone, Copy, Debug)]
struct Tracked {
    id: u32,
    fate: Fate,
    last_ms: u64,
    /// The modifier this key is, for a release by resync.
    modifier: Option<(Mod, Side)>,
}

/// Input typed after a focus-moving input, waiting for the watcher.
#[derive(Clone, Copy, Debug)]
struct Provisional {
    /// Focus epochs started after this epoch decide what follows.
    anchor: u32,
    /// The focus epoch (a stamp) that decided the focus-moving input itself,
    /// if any: records stamped before it were typed after it too.
    floor: Option<u32>,
    /// Key records wait for a focus epoch until then.
    deadline: u64,
    /// The deadline passed with no focus epoch.
    expired: bool,
}

/// The same key identity the processor uses (`Processor::key_slot`): the
/// physical key, or the virtual key when the scan code is unknown.
fn key_id(k: &KeyInput) -> u32 {
    if k.phys.scan != 0 {
        u32::from(k.phys.scan) | (u32::from(k.phys.extended) << 16)
    } else {
        0x8000_0000 | u32::from(k.vk)
    }
}

fn button_id(b: MouseButton) -> u32 {
    0x4000_0000
        | match b {
            MouseButton::Left => 1,
            MouseButton::Right => 2,
            MouseButton::Middle => 3,
            MouseButton::X1 => 4,
            MouseButton::X2 => 5,
        }
}

/// The modifier a key is (Ctrl, Shift, Alt, Win; right Alt counts as Alt).
fn modifier_of(k: &KeyInput) -> Option<(Mod, Side)> {
    match keys::classify(k.vk, k.phys).class {
        KeyClass::Modifier(mk, side) => Some((
            match mk {
                ModKey::Ctrl => Mod::Ctrl,
                ModKey::Shift => Mod::Shift,
                ModKey::Alt | ModKey::AltGr => Mod::Alt,
                ModKey::Win => Mod::Win,
            },
            side,
        )),
        KeyClass::Character | KeyClass::Special => None,
    }
}

/// The receiver side of the hold (see the module documentation).
#[derive(Debug, Default)]
pub struct HoldFilter {
    held: VecDeque<(u32, InputEvent)>,
    /// A `Hold` epoch given up on because too much was held: its records
    /// are discarded.
    abandoned: Option<u32>,
    tracked: Vec<Tracked>,
    provisional: Option<Provisional>,
    /// An odd number of Caps Lock toggles was discarded.
    caps_unsynced: bool,
}

impl HoldFilter {
    pub fn new() -> HoldFilter {
        HoldFilter::default()
    }

    /// True while records are kept back.
    pub fn is_holding(&self) -> bool {
        !self.held.is_empty()
    }

    /// Records kept back right now.
    pub fn held_len(&self) -> usize {
        self.held.len()
    }

    /// True (once) when Caps Lock toggles were discarded since the last
    /// call, so the processor's Caps Lock state may be wrong: read it again
    /// (`GetKeyState(VK_CAPITAL)`) and seed it.
    pub fn take_caps_lock_unsynced(&mut self) -> bool {
        std::mem::take(&mut self.caps_unsynced)
    }

    /// Forgets everything (pause: the queue was emptied, nothing held may
    /// come out afterwards).
    pub fn clear(&mut self) {
        self.held.clear();
        self.abandoned = None;
        self.tracked.clear();
        self.provisional = None;
    }

    /// One record taken from the queue, with its stamp, at `now` (the
    /// clock of the records' `t_ms`). Appends to `out` whatever may be
    /// handed out now; returns how many events it added.
    pub fn push(
        &mut self,
        hold: &PasswordHold,
        stamp: u32,
        ev: InputEvent,
        now: u64,
        out: &mut Vec<InputEvent>,
    ) -> usize {
        if self.held.is_empty() {
            if let Some(d) = self.judge(hold, stamp, &ev, now) {
                return self.emit(stamp, ev, d, out);
            }
        }
        if self.held.len() >= MAX_HELD {
            // Held is empty afterwards, so this recursion ends at once.
            let added = self.abandon(hold, out);
            return added + self.push(hold, stamp, ev, now, out);
        }
        self.held.push_back((stamp, ev));
        0
    }

    /// Hands out (or drops) held records that can be decided at `now`, in
    /// order, up to the first one that cannot. Returns how many events it
    /// added to `out`.
    pub fn flush(&mut self, hold: &PasswordHold, now: u64, out: &mut Vec<InputEvent>) -> usize {
        let mut added = 0;
        while let Some((stamp, ev)) = self.held.pop_front() {
            match self.judge(hold, stamp, &ev, now) {
                Some(d) => added += self.emit(stamp, ev, d, out),
                None => {
                    self.held.push_front((stamp, ev));
                    break;
                }
            }
        }
        added
    }

    /// What a record's own stamp says. `None`: not decided yet.
    fn decide(&self, hold: &PasswordHold, stamp: u32) -> Option<Decision> {
        let (epoch, mode) = decode(stamp);
        match mode {
            HoldMode::Open => Some(Decision::Pass),
            HoldMode::Drop => Some(Decision::Suppressed),
            HoldMode::Hold if self.abandoned == Some(epoch) => Some(Decision::Discard),
            HoldMode::Hold => hold.resolution(stamp).map(Decision::from),
        }
    }

    /// The decision for the next record in stream order, with the
    /// provisional hold applied (module documentation), and the focus epoch
    /// that took part in it. `None`: keep it (and all after it).
    fn judge(
        &mut self,
        hold: &PasswordHold,
        stamp: u32,
        ev: &InputEvent,
        now: u64,
    ) -> Option<(Decision, Option<u32>)> {
        let own = self.decide(hold, stamp);
        let Some(mut p) = self.provisional else {
            return own.map(|d| (d, None));
        };
        let epoch = decode(stamp).0;
        let floor = p.floor.filter(|f| epoch_before(epoch, decode(*f).0));
        let mut focus = None;
        if !p.expired {
            match hold.focus_after(p.anchor) {
                FocusLookup::Found(f) if epoch_before(epoch, decode(f).0) => focus = Some(f),
                FocusLookup::Found(_) => {
                    // Stamped after the watcher saw the focus change: this
                    // record and all after it carry the answer themselves.
                    self.provisional = None;
                    return own.map(|d| (d, None));
                }
                FocusLookup::Lost => {
                    self.provisional = None;
                    return Some((Decision::Discard, None));
                }
                FocusLookup::NotYet if now >= p.deadline => p.expired = true,
                FocusLookup::NotYet => {
                    if matches!(ev, InputEvent::Key(_)) {
                        return None;
                    }
                }
            }
        }
        self.provisional = if p.expired && floor.is_none() {
            None
        } else {
            Some(p)
        };
        let own = own?;
        let mut all_pass = own == Decision::Pass;
        let mut via = None;
        for f in [floor, focus].into_iter().flatten() {
            let d = self.decide(hold, f)?;
            all_pass &= d == Decision::Pass;
            via = Some(f);
        }
        let decision = match via {
            None => own,
            // Decided late, by more than one epoch: the app may no longer
            // suppress by the time these arrive, so nothing passes for
            // bookkeeping either.
            Some(_) if all_pass => Decision::Pass,
            Some(_) => Decision::Discard,
        };
        Some((decision, via))
    }

    /// Too much held: discard everything held (the first held epoch and
    /// whatever is still undecided after it).
    fn abandon(&mut self, hold: &PasswordHold, out: &mut Vec<InputEvent>) -> usize {
        if let Some((stamp, _)) = self.held.front() {
            if decode(*stamp).1 == HoldMode::Hold {
                self.abandoned = Some(decode(*stamp).0);
            }
        }
        self.provisional = None;
        let mut added = 0;
        while let Some((stamp, ev)) = self.held.pop_front() {
            let d = match self.decide(hold, stamp) {
                Some(Decision::Pass) | None => Decision::Discard,
                Some(d) => d,
            };
            added += self.emit(stamp, ev, (d, None), out);
        }
        added
    }

    fn emit(
        &mut self,
        stamp: u32,
        ev: InputEvent,
        (d, via): (Decision, Option<u32>),
        out: &mut Vec<InputEvent>,
    ) -> usize {
        let pass = match &ev {
            InputEvent::Key(k) => match modifier_of(k) {
                Some(m) => return self.emit_modifier(ev, m, d, out),
                None if k.vk == vk::CAPITAL => {
                    // Caps Lock passes while the app suppresses: the
                    // processor keeps its toggle state for the case of
                    // typed letters.
                    let id = key_id(k);
                    let fresh = k.down && self.live(id, k.t_ms).is_none();
                    let drop = d == Decision::Discard;
                    let pass = self.track(id, None, k.down, drop, k.t_ms);
                    if drop && fresh {
                        self.caps_unsynced = !self.caps_unsynced;
                    }
                    pass
                }
                None => self.track(key_id(k), None, k.down, d != Decision::Pass, k.t_ms),
            },
            InputEvent::Button {
                button, down, t_ms, ..
            } => self.track(button_id(*button), None, *down, d != Decision::Pass, *t_ms),
            InputEvent::Wheel { .. } | InputEvent::Move { .. } => d == Decision::Pass,
            InputEvent::ModifierResync { .. } => true,
        };
        if !pass {
            return 0;
        }
        if moves_focus(&ev) {
            // What follows was typed after this input: it answers to the
            // first focus epoch the watcher starts after the one that
            // decided this input (or after its own stamp).
            let (anchor, floor) = match via {
                Some(f) => (decode(f).0, Some(f)),
                None => (decode(stamp).0, None),
            };
            self.provisional = Some(Provisional {
                anchor,
                floor,
                deadline: ev.t_ms().saturating_add(PROVISIONAL_MS),
                expired: false,
            });
        }
        out.push(ev);
        1
    }

    /// A modifier key record (module documentation).
    fn emit_modifier(
        &mut self,
        ev: InputEvent,
        m: (Mod, Side),
        d: Decision,
        out: &mut Vec<InputEvent>,
    ) -> usize {
        let InputEvent::Key(k) = &ev else {
            return 0;
        };
        let (id, down, t) = (key_id(k), k.down, k.t_ms);
        let pos = self.tracked.iter().position(|e| e.id == id);
        let live = self.live(id, t);
        if d == Decision::Discard {
            if down {
                // A fresh press is swallowed; an auto-repeat of a press
                // handed out keeps that fate (its release must still undo
                // it), but is not handed out.
                let fate = if live == Some(Fate::Passed) {
                    Fate::Passed
                } else {
                    Fate::Swallowed
                };
                self.remember(pos, id, fate, t, Some(m));
                return 0;
            }
            if let Some(i) = pos {
                self.tracked.swap_remove(i);
            }
            if live != Some(Fate::Passed) {
                return 0;
            }
            // The processor holds it from a press handed out earlier.
            // Release it with a resync: a key-up here could complete a
            // modifier-tap callout, a resync never does.
            let held = self.passed_modifiers(t);
            out.push(InputEvent::ModifierResync { held, t_ms: t });
            return 1;
        }
        if down {
            if live == Some(Fate::Swallowed) {
                // An auto-repeat of a discarded press: the processor never
                // saw the press, so this would show as one.
                self.remember(pos, id, Fate::Swallowed, t, Some(m));
                return 0;
            }
            self.remember(pos, id, Fate::Passed, t, Some(m));
        } else if let Some(i) = pos {
            // A release always passes: for a modifier the processor does
            // not hold it changes nothing.
            self.tracked.swap_remove(i);
        }
        out.push(ev);
        1
    }

    /// The modifiers whose press was handed out and not yet released.
    fn passed_modifiers(&self, t: u64) -> Vec<(Mod, Side)> {
        let mut held: Vec<(Mod, Side)> = self
            .tracked
            .iter()
            .filter(|e| e.fate == Fate::Passed && t.saturating_sub(e.last_ms) <= STALE_MS)
            .filter_map(|e| e.modifier)
            .collect();
        held.sort_by_key(|(m, s)| (*m as u8, *s as u8));
        held.dedup();
        held
    }

    /// The fate of the last press of `id`, unless it went stale.
    fn live(&self, id: u32, t: u64) -> Option<Fate> {
        self.tracked
            .iter()
            .find(|e| e.id == id)
            .filter(|e| t.saturating_sub(e.last_ms) <= STALE_MS)
            .map(|e| e.fate)
    }

    /// Decides one press or release of a key or button and remembers its
    /// fate. Returns whether it is handed out.
    fn track(
        &mut self,
        id: u32,
        modifier: Option<(Mod, Side)>,
        down: bool,
        drop: bool,
        t: u64,
    ) -> bool {
        let pos = self.tracked.iter().position(|e| e.id == id);
        let live = self.live(id, t);
        if down {
            let fate = match live {
                // An auto-repeat of a dropped press stays dropped.
                Some(Fate::Swallowed) => Fate::Swallowed,
                // An auto-repeat of a handed-out press: dropped while
                // dropping, but its release still goes out.
                Some(Fate::Passed) => {
                    self.remember(pos, id, Fate::Passed, t, modifier);
                    return !drop;
                }
                None if drop => Fate::Swallowed,
                None => Fate::Passed,
            };
            self.remember(pos, id, fate, t, modifier);
            fate == Fate::Passed
        } else {
            if let Some(i) = pos {
                self.tracked.swap_remove(i);
            }
            match live {
                Some(Fate::Swallowed) => false,
                Some(Fate::Passed) => true,
                // A release with no press seen: the processor would show it
                // as a press, so not while dropping.
                None => !drop,
            }
        }
    }

    fn remember(
        &mut self,
        pos: Option<usize>,
        id: u32,
        fate: Fate,
        t: u64,
        modifier: Option<(Mod, Side)>,
    ) {
        let entry = Tracked {
            id,
            fate,
            last_ms: t,
            modifier,
        };
        match pos.and_then(|i| self.tracked.get_mut(i)) {
            Some(e) => *e = entry,
            None => {
                if self.tracked.len() >= MAX_TRACKED {
                    if let Some(oldest) = self
                        .tracked
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, e)| e.last_ms)
                        .map(|(i, _)| i)
                    {
                        self.tracked.swap_remove(oldest);
                    }
                }
                self.tracked.push(entry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::PhysKey;

    fn key(code: u16, scan: u16, down: bool, t: u64) -> InputEvent {
        InputEvent::Key(KeyInput {
            vk: code,
            phys: PhysKey::new(scan, false),
            down,
            injected: false,
            t_ms: t,
        })
    }

    fn a(down: bool, t: u64) -> InputEvent {
        key(0x41, 0x1E, down, t)
    }

    fn p(down: bool, t: u64) -> InputEvent {
        key(0x50, 0x19, down, t)
    }

    fn tab(down: bool, t: u64) -> InputEvent {
        key(vk::TAB, 0x0F, down, t)
    }

    fn shift(down: bool, t: u64) -> InputEvent {
        key(vk::LSHIFT, 0x2A, down, t)
    }

    fn btn(down: bool, t: u64) -> InputEvent {
        InputEvent::Button {
            button: MouseButton::Left,
            down,
            x: 0,
            y: 0,
            injected: false,
            t_ms: t,
        }
    }

    fn keys_of(out: &[InputEvent]) -> Vec<(u16, bool)> {
        out.iter()
            .filter_map(|e| match e {
                InputEvent::Key(k) => Some((k.vk, k.down)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn words_round_trip_and_epochs_advance() {
        let h = PasswordHold::new();
        assert_eq!(decode(h.stamp()), (0, HoldMode::Open));
        let w = h.publish(HoldMode::Hold, Outcome::Drop, true);
        assert_eq!(decode(w), (1, HoldMode::Hold));
        assert_eq!(h.mode(), HoldMode::Hold);
        assert_eq!(h.resolution(w), None, "still current");
        let w2 = h.publish(HoldMode::Open, Outcome::Release, false);
        assert_eq!(decode(w2), (2, HoldMode::Open));
        assert_eq!(h.resolution(w), Some(Outcome::Release));
        h.publish(HoldMode::Hold, Outcome::Drop, true);
        let w4 = h.publish(HoldMode::Drop, Outcome::Drop, false);
        assert_eq!(h.resolution(encode(3, HoldMode::Hold)), Some(Outcome::Drop));
        assert_eq!(decode(w4).1, HoldMode::Drop);
        h.publish(HoldMode::Hold, Outcome::Drop, true);
        h.publish(HoldMode::Hold, Outcome::Discard, true);
        assert_eq!(
            h.resolution(encode(5, HoldMode::Hold)),
            Some(Outcome::Discard)
        );
        // Focus epochs are found after an anchor; plain ones are skipped.
        assert_eq!(
            h.focus_after(1),
            FocusLookup::Found(encode(3, HoldMode::Hold))
        );
        assert_eq!(h.focus_after(6), FocusLookup::NotYet);
        // An epoch whose resolution was overwritten is discarded, and the
        // focus history no longer reaches back that far.
        for _ in 0..(RING * 2) {
            h.publish(HoldMode::Hold, Outcome::Release, false);
        }
        assert_eq!(h.resolution(w), Some(Outcome::Discard));
        assert_eq!(h.focus_after(1), FocusLookup::Lost);
        // Unknown mode bits drop.
        assert_eq!(decode(3).1, HoldMode::Drop);
        assert!(epoch_before(EPOCH_MASK, 0), "epochs wrap");
        assert!(!epoch_before(4, 4));
    }

    #[test]
    fn open_records_pass_and_drop_records_do_not() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let open = h.stamp();
        assert_eq!(f.push(&h, open, a(true, 1), 1, &mut out), 1);
        assert_eq!(f.push(&h, open, a(false, 2), 2, &mut out), 1);
        let drop = h.publish(HoldMode::Drop, Outcome::Drop, false);
        assert_eq!(f.push(&h, drop, p(true, 3), 3, &mut out), 0);
        assert_eq!(f.push(&h, drop, p(false, 4), 4, &mut out), 0);
        let mv = InputEvent::Move {
            x: 1,
            y: 2,
            t_ms: 5,
        };
        assert_eq!(f.push(&h, drop, mv.clone(), 5, &mut out), 0);
        assert_eq!(f.push(&h, open, mv, 5, &mut out), 1);
        assert_eq!(keys_of(&out), vec![(0x41, true), (0x41, false)]);
    }

    /// The finding PRIV-1 scenario: Tab into a password field, the
    /// password typed before UI Automation answers, then a late "password"
    /// verdict. Nothing typed may come out.
    #[test]
    fn keys_held_during_a_focus_change_are_dropped_on_a_late_password_verdict() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        // Tab is pressed in the user name field (shown).
        f.push(&h, h.stamp(), tab(true, 10), 10, &mut out);
        // The focus moved: the watcher starts a hold.
        let hold = h.publish(HoldMode::Hold, Outcome::Drop, true);
        f.push(&h, hold, tab(false, 12), 12, &mut out);
        for t in [20, 30, 40] {
            assert_eq!(f.push(&h, hold, p(true, t), t, &mut out), 0);
            assert_eq!(f.push(&h, hold, p(false, t + 5), t + 5, &mut out), 0);
        }
        assert!(f.is_holding());
        assert_eq!(f.flush(&h, 50, &mut out), 0, "unresolved: still held");
        // UI Automation says "password".
        let drop = h.publish(HoldMode::Drop, Outcome::Drop, false);
        f.push(&h, drop, key(0x0D, 0x1C, true, 60), 60, &mut out);
        assert_eq!(f.flush(&h, 60, &mut out), 1, "only the Tab release");
        assert!(!f.is_holding());
        assert_eq!(keys_of(&out), vec![(0x09, true), (0x09, false)]);
    }

    #[test]
    fn keys_held_during_a_focus_change_are_released_in_order_when_not_a_password() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let hold = h.publish(HoldMode::Hold, Outcome::Drop, true);
        f.push(&h, hold, shift(true, 1), 1, &mut out);
        f.push(&h, hold, a(true, 2), 2, &mut out);
        f.push(&h, hold, shift(false, 3), 3, &mut out);
        assert!(
            out.is_empty(),
            "everything waits, modifiers included, in order"
        );
        let open = h.publish(HoldMode::Open, Outcome::Release, false);
        // A record stamped after the answer queues behind the held ones.
        f.push(&h, open, a(false, 4), 4, &mut out);
        assert!(out.is_empty());
        assert_eq!(f.flush(&h, 4, &mut out), 4);
        assert_eq!(
            keys_of(&out),
            vec![
                (vk::LSHIFT, true),
                (0x41, true),
                (vk::LSHIFT, false),
                (0x41, false)
            ]
        );
    }

    #[test]
    fn a_hold_superseded_by_a_newer_focus_change_is_dropped() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let first = h.publish(HoldMode::Hold, Outcome::Drop, true);
        f.push(&h, first, p(true, 1), 1, &mut out);
        // A newer focus change before the first was classified.
        let second = h.publish(HoldMode::Hold, Outcome::Discard, true);
        f.push(&h, second, a(true, 2), 2, &mut out);
        assert_eq!(f.flush(&h, 2, &mut out), 0, "the second hold is unresolved");
        h.publish(HoldMode::Open, Outcome::Release, false);
        assert_eq!(f.flush(&h, 3, &mut out), 1);
        assert_eq!(keys_of(&out), vec![(0x41, true)]);
    }

    /// PRIV-R3-2: a hold that ends without the app suppressing (a newer
    /// focus change before the verdict) must not hand out the modifiers or
    /// Caps Lock typed in it: the processor would light and sound them and
    /// make "Shift" tap callouts.
    #[test]
    fn a_discarded_hold_drops_modifiers_and_caps_lock_too() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let hold = h.publish(HoldMode::Hold, Outcome::Drop, true);
        let caps = |down, t| key(vk::CAPITAL, 0x3A, down, t);
        for (i, ev) in [
            shift(true, 1),
            p(true, 2),
            p(false, 3),
            shift(false, 4),
            caps(true, 5),
            caps(false, 6),
            shift(true, 7),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(f.push(&h, hold, ev, 1 + i as u64, &mut out), 0);
        }
        // The focus moved on before UI Automation answered.
        let next = h.publish(HoldMode::Hold, Outcome::Discard, true);
        assert_eq!(f.flush(&h, 8, &mut out), 0, "nothing, Shift included");
        assert!(f.take_caps_lock_unsynced(), "a Caps Lock toggle was lost");
        assert!(!f.take_caps_lock_unsynced(), "reported once");
        // Shift is still held: its auto-repeat and release stay dropped,
        // even once the new focus is released.
        f.push(&h, next, shift(true, 30), 30, &mut out);
        h.publish(HoldMode::Open, Outcome::Release, false);
        let open = h.stamp();
        assert_eq!(f.flush(&h, 31, &mut out), 0);
        assert_eq!(f.push(&h, open, shift(true, 60), 60, &mut out), 0);
        assert_eq!(
            f.push(&h, open, shift(false, 70), 70, &mut out),
            1,
            "harmless"
        );
        assert_eq!(f.push(&h, open, shift(true, 80), 80, &mut out), 1, "fresh");
        assert_eq!(keys_of(&out), vec![(vk::LSHIFT, false), (vk::LSHIFT, true)]);
    }

    /// A modifier pressed (and shown) before a discarded hold and released
    /// inside it is released by a resync, which never makes a tap callout.
    #[test]
    fn a_shown_modifier_released_in_a_discarded_hold_is_released_by_resync() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let ctrl = |down, t| key(vk::LCONTROL, 0x1D, down, t);
        assert_eq!(f.push(&h, h.stamp(), shift(true, 1), 1, &mut out), 1);
        assert_eq!(f.push(&h, h.stamp(), ctrl(true, 2), 2, &mut out), 1);
        let hold = h.publish(HoldMode::Hold, Outcome::Drop, true);
        f.push(&h, hold, p(true, 3), 3, &mut out);
        f.push(&h, hold, p(false, 4), 4, &mut out);
        f.push(&h, hold, shift(false, 5), 5, &mut out);
        h.publish(HoldMode::Hold, Outcome::Discard, true);
        assert_eq!(f.flush(&h, 6, &mut out), 1);
        assert_eq!(
            out.last(),
            Some(&InputEvent::ModifierResync {
                held: vec![(Mod::Ctrl, Side::Left)],
                t_ms: 5
            })
        );
        assert_eq!(
            keys_of(&out),
            vec![(vk::LSHIFT, true), (vk::LCONTROL, true)],
            "no Shift key-up"
        );
    }

    #[test]
    fn modifiers_and_caps_lock_pass_while_the_app_suppresses() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let drop = h.publish(HoldMode::Drop, Outcome::Drop, false);
        assert_eq!(f.push(&h, drop, shift(true, 1), 1, &mut out), 1);
        assert_eq!(f.push(&h, drop, p(true, 2), 2, &mut out), 0);
        let caps = key(vk::CAPITAL, 0x3A, true, 3);
        assert_eq!(f.push(&h, drop, caps, 3, &mut out), 1, "Caps Lock state");
        assert!(!f.take_caps_lock_unsynced());
        // Focus leaves the field while P is still down.
        let open = h.publish(HoldMode::Open, Outcome::Drop, true);
        // Its auto-repeat and its release stay dropped: the processor would
        // show a release with no press as a press.
        assert_eq!(f.push(&h, open, p(true, 30), 30, &mut out), 0);
        assert_eq!(f.push(&h, open, p(false, 40), 40, &mut out), 0);
        assert_eq!(f.push(&h, open, shift(false, 41), 41, &mut out), 1);
        // The next press of P is ordinary again.
        assert_eq!(f.push(&h, open, p(true, 50), 50, &mut out), 1);
        assert_eq!(f.push(&h, open, p(false, 60), 60, &mut out), 1);
        assert_eq!(
            keys_of(&out),
            vec![
                (vk::LSHIFT, true),
                (vk::CAPITAL, true),
                (vk::LSHIFT, false),
                (0x50, true),
                (0x50, false)
            ]
        );
    }

    #[test]
    fn a_press_handed_out_before_the_password_field_releases_normally() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        assert_eq!(f.push(&h, h.stamp(), a(true, 1), 1, &mut out), 1);
        let drop = h.publish(HoldMode::Drop, Outcome::Drop, false);
        // Its auto-repeat is not shown, its release is (it reveals nothing
        // and keeps the processor's key state right).
        assert_eq!(f.push(&h, drop, a(true, 30), 30, &mut out), 0);
        assert_eq!(f.push(&h, drop, a(false, 40), 40, &mut out), 1);
        // A release with no press seen is not handed out while dropping.
        assert_eq!(f.push(&h, drop, p(false, 50), 50, &mut out), 0);
    }

    #[test]
    fn mouse_buttons_follow_the_same_rules() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        // Click into the field: the press is shown, the release comes after
        // the focus change and is held, then the field is a password.
        assert_eq!(f.push(&h, h.stamp(), btn(true, 1), 1, &mut out), 1);
        let hold = h.publish(HoldMode::Hold, Outcome::Drop, true);
        assert_eq!(f.push(&h, hold, btn(false, 5), 5, &mut out), 0);
        h.publish(HoldMode::Drop, Outcome::Drop, false);
        assert_eq!(f.flush(&h, 6, &mut out), 1, "the release of a shown press");
        let drop = h.stamp();
        assert_eq!(f.push(&h, drop, btn(true, 10), 10, &mut out), 0);
        let open = h.publish(HoldMode::Open, Outcome::Drop, true);
        assert_eq!(f.push(&h, open, btn(false, 20), 20, &mut out), 0);
        assert_eq!(f.push(&h, open, btn(true, 30), 30, &mut out), 1);
    }

    #[test]
    fn too_much_held_discards_the_held_epoch() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let hold = h.publish(HoldMode::Hold, Outcome::Drop, true);
        for i in 0..MAX_HELD {
            f.push(
                &h,
                hold,
                InputEvent::Move {
                    x: i as i32,
                    y: 0,
                    t_ms: i as u64,
                },
                i as u64,
                &mut out,
            );
        }
        assert_eq!(f.held_len(), MAX_HELD);
        f.push(&h, hold, a(true, 9_000), 9_000, &mut out);
        assert!(out.is_empty(), "abandoned epoch: dropped");
        assert!(!f.is_holding());
        // A later release of the epoch does not bring them back.
        let open = h.publish(HoldMode::Open, Outcome::Release, false);
        assert_eq!(f.flush(&h, 9_001, &mut out), 0);
        assert_eq!(
            f.push(&h, open, a(false, 9_001), 9_001, &mut out),
            0,
            "swallowed"
        );
        f.clear();
        assert_eq!(f.push(&h, open, a(true, 9_002), 9_002, &mut out), 1);
    }

    #[test]
    fn stale_tracking_does_not_eat_a_fresh_press() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let drop = h.publish(HoldMode::Drop, Outcome::Drop, false);
        f.push(&h, drop, p(true, 0), 0, &mut out);
        // Its release was lost (the hooks were out); much later P again.
        let open = h.publish(HoldMode::Open, Outcome::Drop, true);
        let late = STALE_MS + 1;
        assert_eq!(f.push(&h, open, p(true, late), late, &mut out), 1);
        assert_eq!(f.push(&h, open, p(false, late + 1), late + 1, &mut out), 1);
    }

    /// PRIV-R3-1: keys hooked after a Tab but before the app moved the
    /// focus (and before the watcher heard of it) carry the old, open
    /// stamp. They wait for the focus epoch and follow its verdict.
    #[test]
    fn keys_typed_right_after_a_tab_follow_the_focus_it_moved_to() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let open = h.stamp();
        assert_eq!(f.push(&h, open, tab(true, 0), 0, &mut out), 1, "shown");
        // Auto-type: the password follows 10 ms per key, all stamped open.
        f.push(&h, open, tab(false, 1), 1, &mut out);
        for ev in [shift(true, 10), p(true, 11), p(false, 12), shift(false, 13)] {
            let t = ev.t_ms();
            assert_eq!(f.push(&h, open, ev, t, &mut out), 0, "kept back");
        }
        assert_eq!(f.flush(&h, 20, &mut out), 0);
        // The watcher hears of the focus change, then UI Automation says
        // "password" (the app was told to suppress first).
        h.publish(HoldMode::Hold, Outcome::Drop, true);
        assert_eq!(f.flush(&h, 30, &mut out), 0, "waits for the verdict");
        h.publish(HoldMode::Drop, Outcome::Drop, false);
        assert_eq!(f.flush(&h, 60, &mut out), 1, "the Tab release");
        assert_eq!(keys_of(&out), vec![(vk::TAB, true), (vk::TAB, false)]);
        assert!(!f.is_holding());
        // Long after the deadline nothing comes back.
        assert_eq!(f.flush(&h, 10_000, &mut out), 0);
    }

    #[test]
    fn keys_after_a_tab_into_an_ordinary_field_come_out_with_its_verdict() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let open = h.stamp();
        f.push(&h, open, tab(true, 0), 0, &mut out);
        f.push(&h, open, a(true, 5), 5, &mut out);
        f.push(&h, open, a(false, 6), 6, &mut out);
        h.publish(HoldMode::Hold, Outcome::Drop, true);
        let released = h.publish(HoldMode::Open, Outcome::Release, false);
        f.push(&h, released, p(true, 9), 9, &mut out);
        // Well before the deadline: the verdict decides at once.
        assert_eq!(f.flush(&h, 10, &mut out), 3);
        assert_eq!(
            keys_of(&out),
            vec![(vk::TAB, true), (0x41, true), (0x41, false), (0x50, true)]
        );
    }

    #[test]
    fn keys_after_a_tab_that_moves_no_focus_wait_only_until_the_deadline() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let open = h.stamp();
        f.push(&h, open, tab(true, 100), 100, &mut out);
        f.push(&h, open, a(true, 120), 120, &mut out);
        assert_eq!(f.flush(&h, 100 + PROVISIONAL_MS - 1, &mut out), 0);
        assert_eq!(f.flush(&h, 100 + PROVISIONAL_MS, &mut out), 1);
        // Later keys are not held any more.
        assert_eq!(f.push(&h, open, a(false, 400), 400, &mut out), 1);
        assert!(!f.is_holding());
    }

    #[test]
    fn mouse_input_after_a_click_is_not_delayed() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let open = h.stamp();
        assert_eq!(f.push(&h, open, btn(true, 0), 0, &mut out), 1);
        let mv = InputEvent::Move {
            x: 3,
            y: 4,
            t_ms: 2,
        };
        assert_eq!(f.push(&h, open, mv, 2, &mut out), 1);
        assert_eq!(f.push(&h, open, btn(false, 3), 3, &mut out), 1);
        // A key typed right after the click waits; mouse input behind it
        // keeps its place in line.
        assert_eq!(f.push(&h, open, a(true, 20), 20, &mut out), 0);
        assert_eq!(f.push(&h, open, btn(true, 30), 30, &mut out), 0);
        // The click moved the focus to an excluded app's ordinary field.
        h.publish(HoldMode::Hold, Outcome::Drop, true);
        h.publish(HoldMode::Open, Outcome::Release, false);
        assert_eq!(f.flush(&h, 40, &mut out), 2);
    }

    /// Two focus moves in a row: what follows the second answers to the
    /// focus epoch after the one that answered for the first.
    #[test]
    fn a_second_tab_waits_for_its_own_focus_epoch() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let open = h.stamp();
        f.push(&h, open, tab(true, 0), 0, &mut out);
        f.push(&h, open, a(true, 10), 10, &mut out);
        f.push(&h, open, tab(true, 20), 20, &mut out);
        f.push(&h, open, p(true, 30), 30, &mut out);
        // First focus: the user name field. Second: the password field.
        h.publish(HoldMode::Hold, Outcome::Drop, true);
        h.publish(HoldMode::Open, Outcome::Release, false);
        assert_eq!(f.flush(&h, 40, &mut out), 2, "A and the second Tab");
        assert!(f.is_holding(), "P waits for the second focus epoch");
        assert_eq!(f.flush(&h, 20 + PROVISIONAL_MS - 1, &mut out), 0);
        h.publish(HoldMode::Hold, Outcome::Drop, true);
        h.publish(HoldMode::Drop, Outcome::Drop, false);
        assert_eq!(f.flush(&h, 1_000, &mut out), 0);
        assert!(!f.is_holding());
        assert_eq!(
            keys_of(&out),
            vec![(vk::TAB, true), (0x41, true), (vk::TAB, true)]
        );
    }
}
