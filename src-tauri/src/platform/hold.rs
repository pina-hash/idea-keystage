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
//!   hold *epoch* plus a [`HoldMode`]. It starts a new `Hold` epoch on every
//!   focus or foreground change, and ends it when UI Automation has
//!   classified the new focus: [`Outcome::Release`] for "not a password",
//!   [`Outcome::Drop`] for anything else (a password, an element that could
//!   not be read, no answer in time, a newer focus change).
//! * The hook procedures copy the current word into every record (one
//!   atomic load), so each record carries the state the watcher knew when
//!   the key was pressed, in the order of the hook stream.
//! * The input receiver ([`HoldFilter`]) hands `Open` records out, drops
//!   `Drop` records, and keeps `Hold` records, in order, until their epoch is
//!   resolved. Dropped keys stay dropped until released: a key whose press
//!   was dropped never has its release (or auto-repeat) handed out, which
//!   the processor would otherwise treat as a fresh press. Modifier keys and
//!   Caps Lock always pass, so the processor's modifier and Caps Lock
//!   bookkeeping stays right (they reveal nothing typed, and the app shell
//!   suppresses everything while `ContextEvent::Password` says so).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};

use keystage_core::keys::{self, vk, KeyClass};
use keystage_core::{InputEvent, KeyInput, MouseButton};

/// What the receiver does with records stamped in an epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HoldMode {
    /// No password field has the focus: hand records out.
    Open,
    /// The focus changed and is not classified yet: keep records until the
    /// epoch is resolved.
    Hold,
    /// A password field has the focus, or the focus could not be classified
    /// (fail closed): drop records.
    Drop,
}

/// How a `Hold` epoch ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// The new focus is not a password field: hand the held records out.
    Release,
    /// Anything else: drop them.
    Drop,
}

const MODE_BITS: u32 = 2;
const MODE_MASK: u32 = (1 << MODE_BITS) - 1;
const EPOCH_MASK: u32 = u32::MAX >> MODE_BITS;
/// Resolutions remembered. An epoch older than this many transitions that is
/// still held somewhere is dropped (fail closed).
const RING: usize = 256;

fn encode(epoch: u32, mode: HoldMode) -> u32 {
    let m = match mode {
        HoldMode::Open => 0,
        HoldMode::Hold => 1,
        HoldMode::Drop => 2,
    };
    ((epoch & EPOCH_MASK) << MODE_BITS) | m
}

/// The epoch and mode of a stamp.
pub fn decode(word: u32) -> (u32, HoldMode) {
    let mode = match word & MODE_MASK {
        0 => HoldMode::Open,
        1 => HoldMode::Hold,
        // 3 is never written; anything unexpected drops.
        _ => HoldMode::Drop,
    };
    (word >> MODE_BITS, mode)
}

fn encode_outcome(epoch: u32, outcome: Outcome) -> u32 {
    // Codes 1 and 2, so an untouched slot (0) never matches.
    let o = match outcome {
        Outcome::Release => 1,
        Outcome::Drop => 2,
    };
    ((epoch & EPOCH_MASK) << MODE_BITS) | o
}

/// The hold state shared by the watcher (the one writer), the hook
/// procedures (which stamp records with [`PasswordHold::stamp`]) and the
/// input receiver. Only atomics: safe to read inside a hook procedure.
#[derive(Debug)]
pub struct PasswordHold {
    word: AtomicU32,
    resolved: [AtomicU32; RING],
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
    /// otherwise). Returns the new word.
    ///
    /// Threading: the watcher thread only (single writer). The resolution
    /// is stored before the new word is published, so a reader that sees a
    /// later epoch also sees the resolution.
    pub fn publish(&self, mode: HoldMode, outcome: Outcome) -> u32 {
        let (epoch, current) = decode(self.word.load(Ordering::Acquire));
        if current == HoldMode::Hold {
            if let Some(slot) = self.resolved.get(epoch as usize % RING) {
                slot.store(encode_outcome(epoch, outcome), Ordering::Release);
            }
        }
        let next = encode(epoch.wrapping_add(1), mode);
        self.word.store(next, Ordering::Release);
        next
    }

    /// How the epoch of `stamp` ended: `None` while it is still the current
    /// epoch. An epoch whose resolution is no longer remembered is dropped.
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
        } else {
            Some(Outcome::Drop)
        }
    }
}

/// Records kept back at most; beyond this the held epoch is dropped (fail
/// closed). About 8 seconds of a 1000 Hz mouse.
pub const MAX_HELD: usize = 8_192;

/// A dropped or handed-out press older than this whose release never came
/// (the hooks were out, say) no longer decides what happens to the key.
const STALE_MS: u64 = 2_000;
/// Keys tracked at most (keys held down at the same time).
const MAX_TRACKED: usize = 64;

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

/// Keys that always pass: they type nothing, and the processor keeps their
/// state even while suppressed (modifiers, and Caps Lock for the case of
/// typed text).
fn always_passes(k: &KeyInput) -> bool {
    k.vk == vk::CAPITAL || matches!(keys::classify(k.vk, k.phys).class, KeyClass::Modifier(..))
}

/// The receiver side of the hold (see the module documentation).
#[derive(Debug, Default)]
pub struct HoldFilter {
    held: VecDeque<(u32, InputEvent)>,
    /// A `Hold` epoch given up on because too much was held: its records
    /// are dropped.
    abandoned: Option<u32>,
    tracked: Vec<Tracked>,
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

    /// Forgets everything (pause: the queue was emptied, nothing held may
    /// come out afterwards).
    pub fn clear(&mut self) {
        self.held.clear();
        self.abandoned = None;
        self.tracked.clear();
    }

    /// One record taken from the queue, with its stamp. Appends to `out`
    /// whatever may be handed out now; returns how many events it added.
    pub fn push(
        &mut self,
        hold: &PasswordHold,
        stamp: u32,
        ev: InputEvent,
        out: &mut Vec<InputEvent>,
    ) -> usize {
        if self.held.is_empty() {
            if let Some(drop) = self.decide(hold, stamp) {
                return self.emit(ev, drop, out);
            }
        }
        if self.held.len() >= MAX_HELD {
            // Held is empty afterwards, so this recursion ends at once.
            let added = self.abandon(hold, out);
            return added + self.push(hold, stamp, ev, out);
        }
        self.held.push_back((stamp, ev));
        0
    }

    /// Hands out (or drops) held records whose epoch has been resolved, in
    /// order, up to the first one still unresolved. Returns how many events
    /// it added to `out`.
    pub fn flush(&mut self, hold: &PasswordHold, out: &mut Vec<InputEvent>) -> usize {
        let mut added = 0;
        while let Some((stamp, _)) = self.held.front() {
            let Some(drop) = self.decide(hold, *stamp) else {
                break;
            };
            if let Some((_, ev)) = self.held.pop_front() {
                added += self.emit(ev, drop, out);
            }
        }
        added
    }

    /// `Some(true)`: drop, `Some(false)`: hand out, `None`: keep.
    fn decide(&self, hold: &PasswordHold, stamp: u32) -> Option<bool> {
        let (epoch, mode) = decode(stamp);
        match mode {
            HoldMode::Open => Some(false),
            HoldMode::Drop => Some(true),
            HoldMode::Hold if self.abandoned == Some(epoch) => Some(true),
            HoldMode::Hold => hold.resolution(stamp).map(|o| o == Outcome::Drop),
        }
    }

    /// Too much held: drop the first held epoch and everything after it
    /// that is still held.
    fn abandon(&mut self, hold: &PasswordHold, out: &mut Vec<InputEvent>) -> usize {
        if let Some((stamp, _)) = self.held.front() {
            self.abandoned = Some(decode(*stamp).0);
        }
        let mut added = 0;
        while let Some((stamp, ev)) = self.held.pop_front() {
            let drop = self.decide(hold, stamp).unwrap_or(true);
            added += self.emit(ev, drop, out);
        }
        added
    }

    fn emit(&mut self, ev: InputEvent, drop: bool, out: &mut Vec<InputEvent>) -> usize {
        let pass = match &ev {
            InputEvent::Key(k) if always_passes(k) => true,
            InputEvent::Key(k) => self.track(key_id(k), k.down, drop, k.t_ms),
            InputEvent::Button {
                button, down, t_ms, ..
            } => self.track(button_id(*button), *down, drop, *t_ms),
            InputEvent::Wheel { .. } | InputEvent::Move { .. } => !drop,
            InputEvent::ModifierResync { .. } => true,
        };
        if pass {
            out.push(ev);
            1
        } else {
            0
        }
    }

    /// Decides one press or release of a key or button and remembers its
    /// fate. Returns whether it is handed out.
    fn track(&mut self, id: u32, down: bool, drop: bool, t: u64) -> bool {
        let pos = self.tracked.iter().position(|e| e.id == id);
        let entry = pos.and_then(|i| self.tracked.get(i).copied());
        let live = entry.filter(|e| t.saturating_sub(e.last_ms) <= STALE_MS);
        if down {
            let fate = match live.map(|e| e.fate) {
                // An auto-repeat of a dropped press stays dropped.
                Some(Fate::Swallowed) => Fate::Swallowed,
                // An auto-repeat of a handed-out press: dropped while
                // dropping, but its release still goes out.
                Some(Fate::Passed) => {
                    self.remember(pos, id, Fate::Passed, t);
                    return !drop;
                }
                None if drop => Fate::Swallowed,
                None => Fate::Passed,
            };
            self.remember(pos, id, fate, t);
            fate == Fate::Passed
        } else {
            if let Some(i) = pos {
                self.tracked.swap_remove(i);
            }
            match live.map(|e| e.fate) {
                Some(Fate::Swallowed) => false,
                Some(Fate::Passed) => true,
                // A release with no press seen: the processor would show it
                // as a press, so not while dropping.
                None => !drop,
            }
        }
    }

    fn remember(&mut self, pos: Option<usize>, id: u32, fate: Fate, t: u64) {
        let entry = Tracked {
            id,
            fate,
            last_ms: t,
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

    fn shift(down: bool, t: u64) -> InputEvent {
        key(vk::LSHIFT, 0x2A, down, t)
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
        let w = h.publish(HoldMode::Hold, Outcome::Drop);
        assert_eq!(decode(w), (1, HoldMode::Hold));
        assert_eq!(h.mode(), HoldMode::Hold);
        assert_eq!(h.resolution(w), None, "still current");
        let w2 = h.publish(HoldMode::Open, Outcome::Release);
        assert_eq!(decode(w2), (2, HoldMode::Open));
        assert_eq!(h.resolution(w), Some(Outcome::Release));
        h.publish(HoldMode::Hold, Outcome::Drop);
        let w4 = h.publish(HoldMode::Drop, Outcome::Drop);
        assert_eq!(h.resolution(encode(3, HoldMode::Hold)), Some(Outcome::Drop));
        assert_eq!(decode(w4).1, HoldMode::Drop);
        // An epoch whose resolution was overwritten is dropped.
        for _ in 0..(RING * 2) {
            h.publish(HoldMode::Hold, Outcome::Release);
        }
        assert_eq!(h.resolution(w), Some(Outcome::Drop));
        // Unknown mode bits drop.
        assert_eq!(decode(3).1, HoldMode::Drop);
    }

    #[test]
    fn open_records_pass_and_drop_records_do_not() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let open = h.stamp();
        assert_eq!(f.push(&h, open, a(true, 1), &mut out), 1);
        assert_eq!(f.push(&h, open, a(false, 2), &mut out), 1);
        let drop = h.publish(HoldMode::Drop, Outcome::Drop);
        assert_eq!(f.push(&h, drop, p(true, 3), &mut out), 0);
        assert_eq!(f.push(&h, drop, p(false, 4), &mut out), 0);
        let mv = InputEvent::Move {
            x: 1,
            y: 2,
            t_ms: 5,
        };
        assert_eq!(f.push(&h, drop, mv.clone(), &mut out), 0);
        assert_eq!(f.push(&h, open, mv, &mut out), 1);
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
        f.push(&h, h.stamp(), key(0x09, 0x0F, true, 10), &mut out);
        // The focus moved: the watcher starts a hold.
        let hold = h.publish(HoldMode::Hold, Outcome::Drop);
        f.push(&h, hold, key(0x09, 0x0F, false, 12), &mut out);
        for t in [20, 30, 40] {
            assert_eq!(f.push(&h, hold, p(true, t), &mut out), 0);
            assert_eq!(f.push(&h, hold, p(false, t + 5), &mut out), 0);
        }
        assert!(f.is_holding());
        assert_eq!(f.flush(&h, &mut out), 0, "unresolved: still held");
        // UI Automation says "password".
        let drop = h.publish(HoldMode::Drop, Outcome::Drop);
        f.push(&h, drop, key(0x0D, 0x1C, true, 60), &mut out);
        assert_eq!(f.flush(&h, &mut out), 1, "only the Tab release");
        assert!(!f.is_holding());
        assert_eq!(keys_of(&out), vec![(0x09, true), (0x09, false)]);
    }

    #[test]
    fn keys_held_during_a_focus_change_are_released_in_order_when_not_a_password() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let hold = h.publish(HoldMode::Hold, Outcome::Drop);
        f.push(&h, hold, shift(true, 1), &mut out);
        f.push(&h, hold, a(true, 2), &mut out);
        f.push(&h, hold, shift(false, 3), &mut out);
        assert!(
            out.is_empty(),
            "everything waits, modifiers included, in order"
        );
        let open = h.publish(HoldMode::Open, Outcome::Release);
        // A record stamped after the answer queues behind the held ones.
        f.push(&h, open, a(false, 4), &mut out);
        assert!(out.is_empty());
        assert_eq!(f.flush(&h, &mut out), 4);
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
        let first = h.publish(HoldMode::Hold, Outcome::Drop);
        f.push(&h, first, p(true, 1), &mut out);
        // A newer focus change before the first was classified.
        let second = h.publish(HoldMode::Hold, Outcome::Drop);
        f.push(&h, second, a(true, 2), &mut out);
        assert_eq!(f.flush(&h, &mut out), 0, "the second hold is unresolved");
        h.publish(HoldMode::Open, Outcome::Release);
        assert_eq!(f.flush(&h, &mut out), 1);
        assert_eq!(keys_of(&out), vec![(0x41, true)]);
    }

    #[test]
    fn a_dropped_press_never_releases_and_modifiers_always_pass() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let drop = h.publish(HoldMode::Drop, Outcome::Drop);
        assert_eq!(f.push(&h, drop, shift(true, 1), &mut out), 1);
        assert_eq!(f.push(&h, drop, p(true, 2), &mut out), 0);
        let caps = key(vk::CAPITAL, 0x3A, true, 3);
        assert_eq!(f.push(&h, drop, caps, &mut out), 1, "Caps Lock state");
        // Focus leaves the field while P is still down.
        let open = h.publish(HoldMode::Open, Outcome::Drop);
        // Its auto-repeat and its release stay dropped: the processor would
        // show a release with no press as a press.
        assert_eq!(f.push(&h, open, p(true, 30), &mut out), 0);
        assert_eq!(f.push(&h, open, p(false, 40), &mut out), 0);
        assert_eq!(f.push(&h, open, shift(false, 41), &mut out), 1);
        // The next press of P is ordinary again.
        assert_eq!(f.push(&h, open, p(true, 50), &mut out), 1);
        assert_eq!(f.push(&h, open, p(false, 60), &mut out), 1);
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
        assert_eq!(f.push(&h, h.stamp(), a(true, 1), &mut out), 1);
        let drop = h.publish(HoldMode::Drop, Outcome::Drop);
        // Its auto-repeat is not shown, its release is (it reveals nothing
        // and keeps the processor's key state right).
        assert_eq!(f.push(&h, drop, a(true, 30), &mut out), 0);
        assert_eq!(f.push(&h, drop, a(false, 40), &mut out), 1);
        // A release with no press seen is not handed out while dropping.
        assert_eq!(f.push(&h, drop, p(false, 50), &mut out), 0);
    }

    #[test]
    fn mouse_buttons_follow_the_same_rules() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let btn = |down, t| InputEvent::Button {
            button: MouseButton::Left,
            down,
            x: 0,
            y: 0,
            injected: false,
            t_ms: t,
        };
        // Click into the field: the press is shown, the release comes after
        // the focus change and is held, then the field is a password.
        assert_eq!(f.push(&h, h.stamp(), btn(true, 1), &mut out), 1);
        let hold = h.publish(HoldMode::Hold, Outcome::Drop);
        assert_eq!(f.push(&h, hold, btn(false, 5), &mut out), 0);
        h.publish(HoldMode::Drop, Outcome::Drop);
        assert_eq!(f.flush(&h, &mut out), 1, "the release of a shown press");
        let drop = h.stamp();
        assert_eq!(f.push(&h, drop, btn(true, 10), &mut out), 0);
        let open = h.publish(HoldMode::Open, Outcome::Drop);
        assert_eq!(f.push(&h, open, btn(false, 20), &mut out), 0);
        assert_eq!(f.push(&h, open, btn(true, 30), &mut out), 1);
    }

    #[test]
    fn too_much_held_drops_the_held_epoch() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let hold = h.publish(HoldMode::Hold, Outcome::Drop);
        for i in 0..MAX_HELD {
            f.push(
                &h,
                hold,
                InputEvent::Move {
                    x: i as i32,
                    y: 0,
                    t_ms: i as u64,
                },
                &mut out,
            );
        }
        assert_eq!(f.held_len(), MAX_HELD);
        f.push(&h, hold, a(true, 9_000), &mut out);
        assert!(out.is_empty(), "abandoned epoch: dropped");
        assert!(!f.is_holding());
        // A later release of the epoch does not bring them back.
        let open = h.publish(HoldMode::Open, Outcome::Release);
        assert_eq!(f.flush(&h, &mut out), 0);
        assert_eq!(f.push(&h, open, a(false, 9_001), &mut out), 0, "swallowed");
        f.clear();
        assert_eq!(f.push(&h, open, a(true, 9_002), &mut out), 1);
    }

    #[test]
    fn stale_tracking_does_not_eat_a_fresh_press() {
        let h = PasswordHold::new();
        let mut f = HoldFilter::new();
        let mut out = Vec::new();
        let drop = h.publish(HoldMode::Drop, Outcome::Drop);
        f.push(&h, drop, p(true, 0), &mut out);
        // Its release was lost (the hooks were out); much later P again.
        let open = h.publish(HoldMode::Open, Outcome::Drop);
        assert_eq!(f.push(&h, open, p(true, STALE_MS + 1), &mut out), 1);
        assert_eq!(f.push(&h, open, p(false, STALE_MS + 2), &mut out), 1);
    }
}
