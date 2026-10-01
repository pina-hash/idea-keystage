//! The processor (ARCHITECTURE.md section 4): turns raw input into what is
//! shown. Every decision about WHAT is shown (combos, repeats, chords, text
//! runs, masking, captions, suppression) is made here, once.
//!
//! The processor is pure: no IO, no threads, no clock. Callers pass
//! monotonic millisecond timestamps in the events and in [`Processor::tick`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;

use crate::event::{
    Callout, CalloutKind, Chip, ChipKind, DisplayEvent, InputEvent, KeyInput, MonitorRect,
    MouseButton, PhysKey, Side, Suppress,
};
use crate::keys::{self, vk, Accelerator, KeyClass, KeyInfo};
use crate::legends::{self, LegendProvider, TextMods};
use crate::modifiers::{ModKey, ModSet, ModSlot, ModifierState};
use crate::profile::{
    normalize_steps, Caption, CaptionIndex, Direction, MouseAction, MouseQuery, Profile, Step,
};
use crate::rules::{DisplayRules, TypingMode};

/// A key-down for a key already down within this time is an auto-repeat.
/// Windows' longest repeat delay is about one second; a longer gap means the
/// key-up was lost and this is a fresh press.
const REPEAT_GAP_MS: u64 = 1200;
/// A masked run re-sends its callout at most this often, so it stays on
/// screen while typing continues without revealing keystroke timing.
const MASKED_KEEPALIVE_MS: u64 = 1000;
/// One wheel notch.
const WHEEL_DELTA: i32 = 120;

/// Counters for diagnostics and tests. Counts only, never key identities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    /// Every event passed to `handle`.
    pub events: u64,
    /// Fresh non-modifier key presses while not suppressed and not hidden.
    pub key_presses: u64,
    /// Of `key_presses`: shown as a callout create or update.
    pub shown: u64,
    /// Of `key_presses`: absorbed into a masked run.
    pub masked: u64,
    /// Fresh modifier presses.
    pub modifier_presses: u64,
    /// Auto-repeats (key-down for a key already down).
    pub repeats: u64,
    /// Presses of an app hotkey (`hidden_combos`).
    pub hidden: u64,
    /// Key and button presses while suppressed.
    pub suppressed: u64,
    /// Injected events dropped because `show_injected` is off.
    pub injected_ignored: u64,
    /// AltGr fake Ctrl events swallowed.
    pub altgr_fake_ctrl: u64,
    /// Key-ups with no key-down, treated as a press (Print Screen).
    pub up_only_presses: u64,
    /// Modifiers or keys released by aging or resync.
    pub released_stale: u64,
    pub callouts_created: u64,
    pub callout_updates: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Produced {
    Callout(u64),
    Text,
    TextEdit,
    Masked,
    ChordStep,
    Closing,
    Hidden,
    Suppressed,
}

#[derive(Clone, Debug)]
struct HeldKey {
    name: String,
    phys: PhysKey,
    last_ms: u64,
    revealed: bool,
    produced: Produced,
}

#[derive(Clone, Debug)]
struct ChordState {
    /// Started by a solo Alt tap (menu access keys).
    alt_tap: bool,
    /// Started by an explicit `chords` leader: accepts any next step.
    explicit: bool,
    steps: Vec<Step>,
}

#[derive(Clone, Debug)]
enum Mode {
    /// Can collapse repeats through `signature`; nothing else joins it.
    Plain,
    /// An open text run.
    Text,
    /// An open masked run.
    Masked { last_emit_ms: u64 },
    /// Waiting for the next chord step.
    Chord(ChordState),
    /// Finished; only repeat collapsing may still apply.
    Closed,
}

#[derive(Clone, Debug)]
struct Current {
    callout: Callout,
    /// Repeat-collapse identity; `None` means it never collapses.
    signature: Option<String>,
    /// Last input that touched it.
    last_ms: u64,
    mode: Mode,
}

#[derive(Clone, Debug)]
struct TapGroup {
    mods: ModSet,
    sides: Vec<(ModKey, Side)>,
    broken: bool,
}

#[derive(Clone, Copy, Debug)]
struct ButtonHold {
    x0: i32,
    y0: i32,
    t0: u64,
    mods: ModSet,
    dragged: bool,
    callout: Option<u64>,
    revealed: bool,
}

#[derive(Clone, Copy, Debug)]
struct ClickRecord {
    button: MouseButton,
    x: i32,
    y: i32,
    t_down: u64,
    callout: Option<u64>,
}

#[derive(Clone, Debug, Default)]
struct MouseState {
    buttons: [Option<ButtonHold>; 5],
    last_click: Option<ClickRecord>,
    acc_x: i32,
    acc_y: i32,
    wheel_last_ms: u64,
}

enum Action {
    ChordStep,
    Callout,
    TextAppend,
    TextEdit,
    CloseRun,
    Masked,
}

/// Turns input events into display events.
pub struct Processor {
    base_rules: DisplayRules,
    rules: DisplayRules,
    hidden: Vec<Accelerator>,
    profile: Option<Arc<Profile>>,
    index: CaptionIndex,
    suppressed: Option<Suppress>,
    monitor: Option<MonitorRect>,
    mods: ModifierState,
    keys: HashMap<u32, HeldKey>,
    /// Keys released by aging whose real key-up may still arrive; that
    /// key-up must not count as a press.
    aged_keys: HashSet<u32>,
    pending_ctrl: Option<KeyInput>,
    altgr_armed: bool,
    fake_ctrl_down: bool,
    caps_lock: bool,
    next_id: u64,
    current: Option<Current>,
    tap: Option<TapGroup>,
    mouse: MouseState,
    last_t: u64,
    stats: Stats,
}

impl Processor {
    pub fn new(rules: DisplayRules) -> Processor {
        let hidden = rules.parsed_hidden_combos();
        Processor {
            base_rules: rules.clone(),
            rules,
            hidden,
            profile: None,
            index: CaptionIndex::default(),
            suppressed: None,
            monitor: None,
            mods: ModifierState::new(),
            keys: HashMap::new(),
            aged_keys: HashSet::new(),
            pending_ctrl: None,
            altgr_armed: false,
            fake_ctrl_down: false,
            caps_lock: false,
            next_id: 1,
            current: None,
            tap: None,
            mouse: MouseState::default(),
            last_t: 0,
            stats: Stats::default(),
        }
    }

    /// Replaces the general rules. The active profile's `display` block is
    /// applied on top.
    pub fn set_rules(&mut self, rules: DisplayRules) {
        self.base_rules = rules;
        self.recompute_rules();
    }

    fn recompute_rules(&mut self) {
        self.rules = match &self.profile {
            Some(p) => p.apply_display(&self.base_rules),
            None => self.base_rules.clone(),
        };
        self.hidden = self.rules.parsed_hidden_combos();
    }

    /// The rules in force (general rules plus the profile's overrides).
    pub fn rules(&self) -> &DisplayRules {
        &self.rules
    }

    /// Switches profile: captions, chord leaders and display overrides.
    /// Setting the same `Arc` again is free.
    pub fn set_profile(&mut self, profile: Arc<Profile>) {
        if let Some(p) = &self.profile {
            if Arc::ptr_eq(p, &profile) {
                return;
            }
        }
        self.index = CaptionIndex::build(&profile, &[]);
        self.profile = Some(profile);
        self.recompute_rules();
        self.current = None;
    }

    /// Back to no profile (general rules, no captions).
    pub fn clear_profile(&mut self) {
        self.profile = None;
        self.index = CaptionIndex::default();
        self.recompute_rules();
        self.current = None;
    }

    pub fn profile(&self) -> Option<&Arc<Profile>> {
        self.profile.as_ref()
    }

    /// Starts or ends suppression. While suppressed, nothing is shown.
    /// Entering and leaving both emit `ModifiersReleased` so no key stays
    /// lit; leaving then re-lights modifiers that are still held.
    #[must_use = "forward the returned events to the hosts"]
    pub fn set_suppressed(&mut self, s: Option<Suppress>) -> Vec<DisplayEvent> {
        let mut out = Vec::new();
        let was = self.suppressed.is_some();
        let will = s.is_some();
        if was == will {
            self.suppressed = s;
            return out;
        }
        if self.pending_ctrl.is_some() {
            // Settle the deferred Ctrl under the old state first.
            let mut scratch = Vec::new();
            self.flush_pending(&legends::NoLegends, &mut scratch);
            if !was {
                out.extend(scratch);
            }
        }
        self.suppressed = s;
        let t = self.last_t;
        self.current = None;
        self.tap = None;
        self.mouse.last_click = None;
        self.mouse.acc_x = 0;
        self.mouse.acc_y = 0;
        for b in self.mouse.buttons.iter_mut().flatten() {
            b.revealed = false;
        }
        for k in self.keys.values_mut() {
            k.revealed = false;
        }
        self.mods.conceal_all();
        out.push(DisplayEvent::ModifiersReleased { t_ms: t });
        if !will {
            for (slot, h) in self.mods.held_slots() {
                if let Some(m) = self.mods.get_mut(slot) {
                    m.revealed = true;
                }
                out.push(self.mod_key_event(slot, h.phys, true, t));
            }
        }
        out
    }

    pub fn suppressed(&self) -> Option<&Suppress> {
        self.suppressed.as_ref()
    }

    /// The chosen monitor for mouse position normalization. Without one,
    /// button and wheel positions are reported as -1 (off screen) and moves
    /// are not sent.
    pub fn set_monitor(&mut self, m: Option<MonitorRect>) {
        self.monitor = m;
    }

    /// Seeds the Caps Lock state (from `GetKeyState(VK_CAPITAL)`), used for
    /// the case of letters in text runs.
    pub fn set_caps_lock(&mut self, on: bool) {
        self.caps_lock = on;
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// True when a held modifier has had no fresh evidence for
    /// `modifier_verify_ms`: the platform layer should read
    /// `GetAsyncKeyState` and send an [`InputEvent::ModifierResync`].
    pub fn needs_resync(&self, now_ms: u64) -> bool {
        self.mods
            .oldest_evidence()
            .is_some_and(|e| now_ms.saturating_sub(e) > self.rules.modifier_verify_ms)
    }

    /// The earliest time at which [`Processor::tick`] would emit something
    /// (a deferred left Ctrl, a slow right drag, a modifier or key to age),
    /// so the pipeline thread can sleep until then. `None`: nothing pending.
    pub fn next_tick_due(&self) -> Option<u64> {
        let mut due: Option<u64> = None;
        let mut at = |t: u64| due = Some(due.map_or(t, |d| d.min(t)));
        if let Some(p) = &self.pending_ctrl {
            at(p.t_ms + 1);
        }
        if let Some(e) = self.mods.oldest_evidence() {
            at(e + self.rules.modifier_stale_ms + 1);
        }
        if let Some(k) = self.keys.values().map(|h| h.last_ms).min() {
            at(k + self.rules.modifier_stale_ms + 1);
        }
        if self.rules.mouse_callouts {
            if let Some(h) = self.mouse.buttons[MouseButton::Right.index()] {
                if h.revealed && h.dragged && h.callout.is_none() {
                    at(h.t0 + self.rules.gesture_max_ms + 1);
                }
            }
        }
        due
    }

    /// Forgets every held key and button (session lock, sleep). Emits
    /// `ModifiersReleased` unless suppressed.
    pub fn release_all(&mut self, t_ms: u64) -> Vec<DisplayEvent> {
        self.last_t = self.last_t.max(t_ms);
        self.pending_ctrl = None;
        self.altgr_armed = false;
        self.fake_ctrl_down = false;
        self.mods.clear();
        self.aged_keys.extend(self.keys.keys().copied());
        self.keys.clear();
        self.tap = None;
        self.mouse = MouseState::default();
        if let Some(c) = &mut self.current {
            c.mode = Mode::Closed;
        }
        if self.visible() {
            vec![DisplayEvent::ModifiersReleased { t_ms }]
        } else {
            Vec::new()
        }
    }

    /// Processes one input event.
    pub fn handle(&mut self, ev: &InputEvent, legends: &dyn LegendProvider) -> Vec<DisplayEvent> {
        let mut out = Vec::new();
        self.handle_into(ev, legends, &mut out);
        out
    }

    /// Like [`Processor::handle`], appending to a buffer the caller reuses.
    pub fn handle_into(
        &mut self,
        ev: &InputEvent,
        legends: &dyn LegendProvider,
        out: &mut Vec<DisplayEvent>,
    ) {
        self.stats.events += 1;
        self.last_t = self.last_t.max(ev.t_ms());
        match ev {
            InputEvent::Key(k) => self.on_key(k, legends, out),
            InputEvent::Button {
                button,
                down,
                x,
                y,
                injected,
                t_ms,
            } => {
                if *injected && !self.rules.show_injected {
                    self.stats.injected_ignored += 1;
                    return;
                }
                self.flush_pending_before(*t_ms, legends, out);
                self.on_button(*button, *down, *x, *y, *t_ms, out);
            }
            InputEvent::Wheel { dx, dy, x, y, t_ms } => {
                self.flush_pending_before(*t_ms, legends, out);
                self.on_wheel(*dx, *dy, *x, *y, *t_ms, out);
            }
            InputEvent::Move { x, y, t_ms } => {
                self.flush_pending_before(*t_ms, legends, out);
                self.on_move(*x, *y, *t_ms, out);
            }
            InputEvent::ModifierResync { held, t_ms } => {
                self.flush_pending(legends, out);
                self.on_resync(held, *t_ms, out);
            }
        }
    }

    /// Advances time: settles a deferred Ctrl, ages modifiers and keys whose
    /// key-up was lost, turns a long right drag into a callout, and closes
    /// runs and chords whose window passed.
    pub fn tick(&mut self, now_ms: u64) -> Vec<DisplayEvent> {
        let mut out = Vec::new();
        self.last_t = self.last_t.max(now_ms);
        if let Some(p) = self.pending_ctrl {
            if now_ms > p.t_ms {
                self.flush_pending(&legends::NoLegends, &mut out);
            }
        }
        let visible = self.visible();
        let aged = self.mods.age(now_ms, self.rules.modifier_stale_ms);
        for (slot, h) in &aged {
            self.stats.released_stale += 1;
            if h.revealed && visible {
                out.push(self.mod_key_event(*slot, h.phys, false, now_ms));
            }
            if *slot == ModSlot::AltGr {
                self.fake_ctrl_down = false;
            }
        }
        if !aged.is_empty() && !self.mods.any_held() {
            self.tap = None;
        }
        let stale = self.rules.modifier_stale_ms;
        let stale_keys: Vec<u32> = self
            .keys
            .iter()
            .filter(|(_, h)| now_ms.saturating_sub(h.last_ms) > stale)
            .map(|(k, _)| *k)
            .collect();
        for slot in stale_keys {
            if let Some(h) = self.keys.remove(&slot) {
                self.stats.released_stale += 1;
                self.aged_keys.insert(slot);
                if h.revealed && visible {
                    out.push(DisplayEvent::Key {
                        phys: h.phys.wire_id(),
                        key: h.name,
                        down: false,
                        modifier: None,
                        t_ms: now_ms,
                    });
                }
            }
        }
        if visible && self.rules.mouse_callouts {
            let i = MouseButton::Right.index();
            if let Some(h) = self.mouse.buttons[i] {
                if h.revealed
                    && h.dragged
                    && h.callout.is_none()
                    && now_ms.saturating_sub(h.t0) > self.rules.gesture_max_ms
                {
                    let id = self.mouse_callout(
                        MouseButton::Right,
                        MouseAction::Drag,
                        h.mods,
                        None,
                        now_ms,
                        &mut out,
                    );
                    if let Some(b) = self.mouse.buttons[i].as_mut() {
                        b.callout = Some(id);
                    }
                }
            }
        }
        let gap = self.rules.text_run_gap_ms;
        let chord_window = self.rules.chord_window_ms;
        if let Some(cur) = &mut self.current {
            let idle = now_ms.saturating_sub(cur.last_ms);
            let expired = match cur.mode {
                Mode::Text | Mode::Masked { .. } => idle > gap,
                Mode::Chord(_) => idle > chord_window,
                Mode::Plain | Mode::Closed => false,
            };
            if expired {
                cur.mode = Mode::Closed;
            }
        }
        out
    }

    fn visible(&self) -> bool {
        self.suppressed.is_none()
    }

    // ---- keyboard ----------------------------------------------------

    fn on_key(&mut self, k: &KeyInput, legends: &dyn LegendProvider, out: &mut Vec<DisplayEvent>) {
        if k.injected && !self.rules.show_injected {
            self.stats.injected_ignored += 1;
            return;
        }
        let info = keys::classify(k.vk, k.phys);
        let is_left_ctrl = matches!(info.class, KeyClass::Modifier(ModKey::Ctrl, Side::Left));
        let is_right_alt_down =
            k.down && matches!(info.class, KeyClass::Modifier(ModKey::Alt, Side::Right));

        // AltGr's fake left Ctrl, by its (undocumented) scan code.
        if is_left_ctrl && keys::is_altgr_fake_ctrl(k.vk, k.phys) {
            self.flush_pending(legends, out);
            self.altgr_armed = k.down;
            self.fake_ctrl_down = k.down;
            self.stats.altgr_fake_ctrl += 1;
            return;
        }
        // Its key-up when it was detected by the timestamp fallback.
        if is_left_ctrl
            && !k.down
            && self.fake_ctrl_down
            && !self.mods.is_held(ModSlot::CtrlL)
            && self.pending_ctrl.is_none()
        {
            self.fake_ctrl_down = false;
            self.stats.altgr_fake_ctrl += 1;
            return;
        }
        // Fallback: a left Ctrl with the same timestamp immediately before a
        // right Alt is AltGr's fake Ctrl.
        if let Some(p) = self.pending_ctrl {
            if is_right_alt_down && k.t_ms == p.t_ms {
                self.pending_ctrl = None;
                self.fake_ctrl_down = true;
                self.altgr_armed = true;
                self.stats.altgr_fake_ctrl += 1;
            } else {
                self.flush_pending(legends, out);
            }
        }
        let armed = std::mem::take(&mut self.altgr_armed) && is_right_alt_down;
        if is_left_ctrl && k.down {
            let fresh = match self.mods.get(ModSlot::CtrlL) {
                Some(h) => k.t_ms.saturating_sub(h.evidence_ms) > REPEAT_GAP_MS,
                None => true,
            };
            if fresh {
                self.pending_ctrl = Some(*k);
                return;
            }
        }
        self.process_key(k, &info, armed, legends, out);
    }

    fn flush_pending_before(
        &mut self,
        t_ms: u64,
        legends: &dyn LegendProvider,
        out: &mut Vec<DisplayEvent>,
    ) {
        if let Some(p) = self.pending_ctrl {
            if t_ms > p.t_ms {
                self.flush_pending(legends, out);
            }
        }
    }

    fn flush_pending(&mut self, legends: &dyn LegendProvider, out: &mut Vec<DisplayEvent>) {
        if let Some(p) = self.pending_ctrl.take() {
            let info = keys::classify(p.vk, p.phys);
            self.process_key(&p, &info, false, legends, out);
        }
    }

    fn process_key(
        &mut self,
        k: &KeyInput,
        info: &KeyInfo,
        altgr: bool,
        legends: &dyn LegendProvider,
        out: &mut Vec<DisplayEvent>,
    ) {
        match info.class {
            KeyClass::Modifier(mk, side) => self.on_modifier(k, mk, side, altgr, out),
            _ if k.down => self.on_press(k, info, legends, out),
            _ => self.on_release(k, info, legends, out),
        }
    }

    fn mod_key_event(&self, slot: ModSlot, phys: PhysKey, down: bool, t_ms: u64) -> DisplayEvent {
        let phys = if phys.scan == 0 {
            keys::modifier_phys(slot.key(), slot.side())
        } else {
            phys
        };
        DisplayEvent::Key {
            phys: phys.wire_id(),
            key: slot.key().name().to_string(),
            down,
            modifier: Some(slot.key().wire()),
            t_ms,
        }
    }

    fn on_modifier(
        &mut self,
        k: &KeyInput,
        mk: ModKey,
        side: Side,
        altgr: bool,
        out: &mut Vec<DisplayEvent>,
    ) {
        let mut slot = ModSlot::of(mk, side);
        if slot == ModSlot::AltR
            && ((k.down && altgr) || (!k.down && self.mods.is_held(ModSlot::AltGr)))
        {
            slot = ModSlot::AltGr;
        }
        let visible = self.visible();
        if k.down {
            if let Some(h) = self.mods.get(slot).copied() {
                if k.t_ms.saturating_sub(h.evidence_ms) <= REPEAT_GAP_MS {
                    self.mods.refresh(slot, k.t_ms);
                    self.stats.repeats += 1;
                    return;
                }
                // Its key-up was lost; this is a fresh press.
                self.mods.release(slot);
                if h.revealed && visible {
                    out.push(self.mod_key_event(slot, h.phys, false, k.t_ms));
                }
            }
            let any_before = self.mods.any_held();
            self.mods.press(slot, k.phys, k.t_ms, visible);
            if !visible {
                self.stats.suppressed += 1;
                self.tap = None;
                return;
            }
            self.stats.modifier_presses += 1;
            out.push(self.mod_key_event(slot, k.phys, true, k.t_ms));
            match (&mut self.tap, any_before) {
                (Some(tap), true) => {
                    tap.mods.insert(slot.key());
                    tap.sides.push((slot.key(), slot.side()));
                }
                (_, false) => {
                    self.tap = Some(TapGroup {
                        mods: ModSet::EMPTY.with(slot.key()),
                        sides: vec![(slot.key(), slot.side())],
                        // A key already held (typing, a game key) means
                        // this modifier is not a clean tap.
                        broken: !self.keys.is_empty(),
                    });
                }
                (None, true) => {}
            }
        } else {
            let Some(h) = self.mods.release(slot) else {
                return;
            };
            if slot == ModSlot::AltGr {
                self.fake_ctrl_down = false;
            }
            if h.revealed && visible {
                out.push(self.mod_key_event(slot, h.phys, false, k.t_ms));
            }
            if !self.mods.any_held() {
                if let Some(tap) = self.tap.take() {
                    if !tap.broken && visible {
                        self.emit_tap(&tap, k.t_ms, out);
                    }
                }
            }
        }
    }

    fn mod_chip(&self, mk: ModKey, side: Option<Side>) -> Chip {
        let label = match (self.rules.show_modifier_side, mk, side) {
            (true, ModKey::AltGr, _) | (false, _, _) | (true, _, None) => mk.name().to_string(),
            (true, _, Some(Side::Left)) => format!("Left {}", mk.name()),
            (true, _, Some(Side::Right)) => format!("Right {}", mk.name()),
        };
        Chip {
            key: mk.name().to_string(),
            label,
            kind: ChipKind::Modifier,
        }
    }

    /// Chips for the currently held modifiers in `mods`.
    fn held_mod_chips(&self, mods: ModSet) -> Vec<Chip> {
        mods.iter()
            .map(|m| self.mod_chip(m, self.mods.side_of(m)))
            .collect()
    }

    fn emit_tap(&mut self, tap: &TapGroup, t: u64, out: &mut Vec<DisplayEvent>) {
        let chips: Vec<Chip> = tap
            .mods
            .iter()
            .map(|m| self.mod_chip(m, tap.sides.iter().find(|(k, _)| *k == m).map(|(_, s)| *s)))
            .collect();
        let kind = if tap.mods.len() == 1 {
            CalloutKind::Key
        } else {
            CalloutKind::Combo
        };
        let step = Step {
            mods: tap.mods,
            key: None,
        };
        let cap = self.index.lookup(std::slice::from_ref(&step)).cloned();
        let sig = signature(std::slice::from_ref(&step));
        self.emit_collapsible(kind, vec![chips], cap, sig, 1, t, out);
        if tap.mods == ModSet::EMPTY.with(ModKey::Alt) {
            if let Some(cur) = &mut self.current {
                cur.mode = Mode::Chord(ChordState {
                    alt_tap: true,
                    explicit: false,
                    steps: vec![step],
                });
                cur.last_ms = t;
            }
        }
    }

    fn is_hidden(&self, mods: ModSet, key: &str) -> bool {
        self.hidden.iter().any(|a| a.mods == mods && a.key == key)
    }

    fn text_run_open(&self, t: u64) -> bool {
        matches!(&self.current, Some(c) if matches!(c.mode, Mode::Text) && t.saturating_sub(c.last_ms) <= self.rules.text_run_gap_ms)
    }

    fn masked_run_open(&self, t: u64) -> bool {
        matches!(&self.current, Some(c) if matches!(c.mode, Mode::Masked { .. }) && t.saturating_sub(c.last_ms) <= self.rules.text_run_gap_ms)
    }

    fn chord_accepts(&self, step: &Step, t: u64) -> bool {
        let Some(cur) = &self.current else {
            return false;
        };
        let Mode::Chord(ch) = &cur.mode else {
            return false;
        };
        if t.saturating_sub(cur.last_ms) > self.rules.chord_window_ms {
            return false;
        }
        if ch.alt_tap {
            return step.mods.is_empty() && (ch.steps.len() as u32) < self.rules.chord_max_steps;
        }
        if ch.explicit && ch.steps.len() == 1 {
            return true;
        }
        let mut cand = ch.steps.clone();
        cand.push(step.clone());
        self.index.lookup(&cand).is_some() || self.index.is_prefix(&cand)
    }

    fn key_slot(k: &KeyInput) -> u32 {
        if k.phys.scan != 0 {
            u32::from(k.phys.scan) | (u32::from(k.phys.extended) << 16)
        } else {
            0x8000_0000 | u32::from(k.vk)
        }
    }

    fn typed_text(
        &self,
        k: &KeyInput,
        mods: ModSet,
        legends: &dyn LegendProvider,
        label: &str,
        name: &str,
    ) -> String {
        let tm = TextMods {
            shift: mods.contains(ModKey::Shift),
            altgr: mods.contains(ModKey::AltGr),
            caps_lock: self.caps_lock,
        };
        match legends.text(k.vk, k.phys, tm) {
            Some(s) if !s.is_empty() && !s.chars().any(|c| c.is_control() && c != '\t') => s,
            _ if name == "Space" => " ".to_string(),
            _ => label.to_string(),
        }
    }

    fn on_press(
        &mut self,
        k: &KeyInput,
        info: &KeyInfo,
        legends: &dyn LegendProvider,
        out: &mut Vec<DisplayEvent>,
    ) {
        let slot_id = Self::key_slot(k);
        let t = k.t_ms;
        self.aged_keys.remove(&slot_id);
        if let Some(h) = self.keys.get_mut(&slot_id) {
            if t.saturating_sub(h.last_ms) <= REPEAT_GAP_MS {
                h.last_ms = t;
                let produced = h.produced;
                self.stats.repeats += 1;
                if self.visible() {
                    self.on_auto_repeat(k, info, produced, legends, out);
                }
                return;
            }
        }
        let visible = self.visible();
        if let Some(old) = self.keys.remove(&slot_id) {
            if old.revealed && visible {
                out.push(DisplayEvent::Key {
                    phys: old.phys.wire_id(),
                    key: old.name,
                    down: false,
                    modifier: None,
                    t_ms: t,
                });
            }
        }
        if let Some(tap) = &mut self.tap {
            tap.broken = true;
        }
        if k.vk == vk::CAPITAL {
            self.caps_lock = !self.caps_lock;
        }
        let name = info.name.to_string();
        let mut held = HeldKey {
            name: name.clone(),
            phys: k.phys,
            last_ms: t,
            revealed: false,
            produced: Produced::Suppressed,
        };
        if !visible {
            self.stats.suppressed += 1;
            self.keys.insert(slot_id, held);
            return;
        }
        let mods = self.mods.held();
        if self.is_hidden(mods, &name) {
            self.stats.hidden += 1;
            held.produced = Produced::Hidden;
            self.keys.insert(slot_id, held);
            return;
        }
        self.stats.key_presses += 1;
        let step = Step {
            mods,
            key: Some(name.clone()),
        };
        let label = legends::chip_label(legends, k.vk, k.phys, &name);
        let action = self.decide(info, &step, &name, t);
        let revealed = !matches!(action, Action::Masked);
        if revealed {
            out.push(DisplayEvent::Key {
                phys: k.phys.wire_id(),
                key: name.clone(),
                down: true,
                modifier: None,
                t_ms: t,
            });
            self.stats.shown += 1;
        } else {
            self.stats.masked += 1;
        }
        held.revealed = revealed;
        let key_chip = Chip {
            key: name.clone(),
            label: label.clone(),
            kind: ChipKind::Key,
        };
        held.produced = match action {
            Action::ChordStep => {
                self.chord_step(step, key_chip, t, out);
                Produced::ChordStep
            }
            Action::Callout => {
                let mut chips = self.held_mod_chips(mods);
                chips.push(key_chip);
                let kind = if mods.is_empty() {
                    CalloutKind::Key
                } else {
                    CalloutKind::Combo
                };
                let cap = self.index.lookup(std::slice::from_ref(&step)).cloned();
                let sig = signature(std::slice::from_ref(&step));
                let id = self.emit_collapsible(kind, vec![chips], cap, sig, 1, t, out);
                let explicit = self.index.is_leader(&step);
                if explicit || self.index.is_prefix(std::slice::from_ref(&step)) {
                    if let Some(cur) = &mut self.current {
                        cur.mode = Mode::Chord(ChordState {
                            alt_tap: false,
                            explicit,
                            steps: vec![step],
                        });
                    }
                }
                Produced::Callout(id)
            }
            Action::TextAppend => {
                let text = self.typed_text(k, mods, legends, &label, &name);
                self.text_append(&text, t, out);
                Produced::Text
            }
            Action::TextEdit => {
                self.text_backspace(t, out);
                Produced::TextEdit
            }
            Action::CloseRun => {
                self.close_run(key_chip, t, out);
                Produced::Closing
            }
            Action::Masked => {
                self.masked(t, out);
                Produced::Masked
            }
        };
        self.keys.insert(slot_id, held);
    }

    fn decide(&self, info: &KeyInfo, step: &Step, name: &str, t: u64) -> Action {
        if self.chord_accepts(step, t) {
            return Action::ChordStep;
        }
        let mods = step.mods;
        let cmd = mods.has_command();
        match info.class {
            KeyClass::Character if !cmd => {
                let typing_shown = self.rules.typing == TypingMode::All && self.rules.typed_values;
                if typing_shown && keys::is_value_key(name) {
                    return Action::TextAppend;
                }
                if self.index.lookup(std::slice::from_ref(step)).is_some() {
                    return Action::Callout;
                }
                if typing_shown {
                    Action::TextAppend
                } else {
                    Action::Masked
                }
            }
            KeyClass::Special if mods.is_empty() => {
                let run = self.text_run_open(t);
                let masked = self.masked_run_open(t);
                if name == "Backspace" && run {
                    Action::TextEdit
                } else if name == "Backspace" && masked {
                    Action::Masked
                } else if (keys::is_enter(name) || name == "Tab") && (run || masked) {
                    Action::CloseRun
                } else {
                    Action::Callout
                }
            }
            _ => Action::Callout,
        }
    }

    fn on_auto_repeat(
        &mut self,
        k: &KeyInput,
        info: &KeyInfo,
        produced: Produced,
        legends: &dyn LegendProvider,
        out: &mut Vec<DisplayEvent>,
    ) {
        let t = k.t_ms;
        match produced {
            Produced::Callout(id) => {
                if let Some(cur) = &mut self.current {
                    if cur.callout.id == id && cur.signature.is_some() {
                        cur.callout.count = cur.callout.count.saturating_add(1);
                        cur.callout.updated_ms = t;
                        cur.last_ms = t;
                        out.push(DisplayEvent::Callout(cur.callout.clone()));
                        self.stats.callout_updates += 1;
                    }
                }
            }
            Produced::Text => {
                if self.text_run_open(t) {
                    let name = info.name.to_string();
                    let label = legends::chip_label(legends, k.vk, k.phys, &name);
                    let text = self.typed_text(k, self.mods.held(), legends, &label, &name);
                    self.text_append(&text, t, out);
                }
            }
            Produced::TextEdit => {
                if self.text_run_open(t) {
                    self.text_backspace(t, out);
                }
            }
            Produced::Masked => {
                if self.masked_run_open(t) {
                    self.masked(t, out);
                }
            }
            Produced::ChordStep | Produced::Closing | Produced::Hidden | Produced::Suppressed => {}
        }
    }

    fn on_release(
        &mut self,
        k: &KeyInput,
        info: &KeyInfo,
        legends: &dyn LegendProvider,
        out: &mut Vec<DisplayEvent>,
    ) {
        let slot_id = Self::key_slot(k);
        let visible = self.visible();
        if let Some(h) = self.keys.remove(&slot_id) {
            if h.revealed && visible {
                out.push(DisplayEvent::Key {
                    phys: h.phys.wire_id(),
                    key: h.name,
                    down: false,
                    modifier: None,
                    t_ms: k.t_ms,
                });
            }
            return;
        }
        if self.aged_keys.remove(&slot_id) || !visible {
            return;
        }
        // A key-up with no key-down (Print Screen) is a press.
        self.stats.up_only_presses += 1;
        let down = KeyInput { down: true, ..*k };
        self.on_press(&down, info, legends, out);
        if let Some(h) = self.keys.remove(&slot_id) {
            if h.revealed {
                out.push(DisplayEvent::Key {
                    phys: h.phys.wire_id(),
                    key: h.name,
                    down: false,
                    modifier: None,
                    t_ms: k.t_ms,
                });
            }
        }
    }

    // ---- callouts -----------------------------------------------------

    fn profile_id(&self) -> Option<String> {
        self.profile.as_ref().map(|p| p.id.clone())
    }

    #[allow(clippy::too_many_arguments)]
    fn new_callout(
        &mut self,
        kind: CalloutKind,
        steps: Vec<Vec<Chip>>,
        text: Option<String>,
        cap: Option<Caption>,
        signature: Option<String>,
        mode: Mode,
        t: u64,
        out: &mut Vec<DisplayEvent>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let (caption, context) = match cap {
            Some(c) => (Some(c.command), c.context),
            None => (None, None),
        };
        let callout = Callout {
            id,
            kind,
            steps,
            text,
            count: 1,
            caption,
            context,
            profile: self.profile_id(),
            t_ms: t,
            updated_ms: t,
        };
        out.push(DisplayEvent::Callout(callout.clone()));
        self.stats.callouts_created += 1;
        self.current = Some(Current {
            callout,
            signature,
            last_ms: t,
            mode,
        });
        id
    }

    /// Adds `add` to the current callout if it has the same signature and
    /// was updated within the repeat window; otherwise starts a new one.
    #[allow(clippy::too_many_arguments)]
    fn emit_collapsible(
        &mut self,
        kind: CalloutKind,
        steps: Vec<Vec<Chip>>,
        cap: Option<Caption>,
        sig: String,
        add: u32,
        t: u64,
        out: &mut Vec<DisplayEvent>,
    ) -> u64 {
        let window = self.rules.repeat_window_ms;
        if let Some(cur) = &mut self.current {
            if cur.signature.as_deref() == Some(sig.as_str())
                && t.saturating_sub(cur.callout.updated_ms) <= window
            {
                cur.callout.count = cur.callout.count.saturating_add(add);
                cur.callout.updated_ms = t;
                cur.last_ms = t;
                out.push(DisplayEvent::Callout(cur.callout.clone()));
                self.stats.callout_updates += 1;
                return cur.callout.id;
            }
        }
        let id = self.new_callout(kind, steps, None, cap, Some(sig), Mode::Plain, t, out);
        if add > 1 {
            if let Some(cur) = &mut self.current {
                cur.callout.count = add;
                if let Some(DisplayEvent::Callout(c)) = out.last_mut() {
                    c.count = add;
                }
            }
        }
        id
    }

    fn chord_step(&mut self, step: Step, key_chip: Chip, t: u64, out: &mut Vec<DisplayEvent>) {
        let mut chips = self.held_mod_chips(step.mods);
        chips.push(key_chip);
        let max_steps = self.rules.chord_max_steps;
        let Some(cur) = &mut self.current else { return };
        let Mode::Chord(ch) = &mut cur.mode else {
            return;
        };
        ch.steps.push(step);
        let norm = normalize_steps(&ch.steps);
        let keep_open = if ch.alt_tap {
            (ch.steps.len() as u32) < max_steps
        } else {
            self.index.is_prefix(&ch.steps)
        };
        let cap = self.index.lookup(&norm).cloned();
        cur.callout.steps.push(chips);
        cur.callout.kind = CalloutKind::Chord;
        cur.callout.caption = cap.as_ref().map(|c| c.command.clone());
        cur.callout.context = cap.and_then(|c| c.context);
        cur.callout.updated_ms = t;
        cur.signature = None;
        cur.last_ms = t;
        if !keep_open {
            cur.mode = Mode::Closed;
        }
        out.push(DisplayEvent::Callout(cur.callout.clone()));
        self.stats.callout_updates += 1;
    }

    fn text_append(&mut self, s: &str, t: u64, out: &mut Vec<DisplayEvent>) {
        if self.text_run_open(t) {
            if let Some(cur) = &mut self.current {
                cur.callout.text.get_or_insert_with(String::new).push_str(s);
                cur.callout.updated_ms = t;
                cur.last_ms = t;
                out.push(DisplayEvent::Callout(cur.callout.clone()));
                self.stats.callout_updates += 1;
                return;
            }
        }
        self.new_callout(
            CalloutKind::Text,
            vec![Vec::new()],
            Some(s.to_string()),
            None,
            None,
            Mode::Text,
            t,
            out,
        );
    }

    fn text_backspace(&mut self, t: u64, out: &mut Vec<DisplayEvent>) {
        if let Some(cur) = &mut self.current {
            if let Some(text) = cur.callout.text.as_mut() {
                text.pop();
            }
            cur.callout.updated_ms = t;
            cur.last_ms = t;
            out.push(DisplayEvent::Callout(cur.callout.clone()));
            self.stats.callout_updates += 1;
        }
    }

    fn close_run(&mut self, chip: Chip, t: u64, out: &mut Vec<DisplayEvent>) {
        if let Some(cur) = &mut self.current {
            cur.callout.steps = vec![vec![chip]];
            cur.callout.updated_ms = t;
            cur.last_ms = t;
            cur.mode = Mode::Closed;
            cur.signature = None;
            out.push(DisplayEvent::Callout(cur.callout.clone()));
            self.stats.callout_updates += 1;
        }
    }

    fn masked(&mut self, t: u64, out: &mut Vec<DisplayEvent>) {
        if self.masked_run_open(t) {
            if let Some(cur) = &mut self.current {
                cur.last_ms = t;
                if let Mode::Masked { last_emit_ms } = &mut cur.mode {
                    if t.saturating_sub(*last_emit_ms) >= MASKED_KEEPALIVE_MS {
                        *last_emit_ms = t;
                        cur.callout.updated_ms = t;
                        out.push(DisplayEvent::Callout(cur.callout.clone()));
                        self.stats.callout_updates += 1;
                    }
                }
                return;
            }
        }
        self.new_callout(
            CalloutKind::Masked,
            vec![Vec::new()],
            None,
            None,
            None,
            Mode::Masked { last_emit_ms: t },
            t,
            out,
        );
    }

    /// Ends text runs, masked runs and chords (a click moved the focus).
    fn end_runs(&mut self) {
        if let Some(cur) = &mut self.current {
            if matches!(cur.mode, Mode::Text | Mode::Masked { .. } | Mode::Chord(_)) {
                cur.mode = Mode::Closed;
            }
        }
    }

    // ---- mouse --------------------------------------------------------

    fn norm(&self, x: i32, y: i32) -> (f32, f32) {
        match &self.monitor {
            Some(m) => m.normalize(x, y),
            None => (-1.0, -1.0),
        }
    }

    fn mouse_shown(&self, button: MouseButton, mods: ModSet) -> bool {
        self.rules.mouse_callouts
            && (self.rules.plain_left_clicks || button != MouseButton::Left || !mods.is_empty())
    }

    fn mouse_callout(
        &mut self,
        button: MouseButton,
        action: MouseAction,
        mods: ModSet,
        direction: Option<Direction>,
        t: u64,
        out: &mut Vec<DisplayEvent>,
    ) -> u64 {
        self.mouse_callout_n(button, action, mods, direction, 1, t, out)
    }

    #[allow(clippy::too_many_arguments)]
    fn mouse_callout_n(
        &mut self,
        button: MouseButton,
        action: MouseAction,
        mods: ModSet,
        direction: Option<Direction>,
        add: u32,
        t: u64,
        out: &mut Vec<DisplayEvent>,
    ) -> u64 {
        let chip = mouse_chip(button, action, direction);
        let mut chips = self.held_mod_chips(mods);
        chips.push(chip.clone());
        let cap = self
            .index
            .lookup_mouse(&MouseQuery {
                button,
                action,
                mods,
                direction,
            })
            .cloned();
        let sig = format!("mouse:{}:{}", mods.bits(), chip.key);
        self.emit_collapsible(CalloutKind::Mouse, vec![chips], cap, sig, add, t, out)
    }

    fn on_button(
        &mut self,
        button: MouseButton,
        down: bool,
        x: i32,
        y: i32,
        t: u64,
        out: &mut Vec<DisplayEvent>,
    ) {
        let i = button.index();
        let visible = self.visible();
        if down {
            if let Some(tap) = &mut self.tap {
                tap.broken = true;
            }
            if !visible {
                self.stats.suppressed += 1;
            } else {
                self.end_runs();
            }
            self.mouse.buttons[i] = Some(ButtonHold {
                x0: x,
                y0: y,
                t0: t,
                mods: self.mods.held(),
                dragged: false,
                callout: None,
                revealed: visible,
            });
            if visible {
                let (nx, ny) = self.norm(x, y);
                out.push(DisplayEvent::Button {
                    button,
                    down: true,
                    x: nx,
                    y: ny,
                    t_ms: t,
                });
            }
            return;
        }
        let Some(mut h) = self.mouse.buttons[i].take() else {
            return;
        };
        if !(h.revealed && visible) {
            return;
        }
        let (nx, ny) = self.norm(x, y);
        out.push(DisplayEvent::Button {
            button,
            down: false,
            x: nx,
            y: ny,
            t_ms: t,
        });
        let dx = f64::from(x - h.x0);
        let dy = f64::from(y - h.y0);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > f64::from(self.rules.drag_threshold_px) {
            h.dragged = true;
        }
        if !self.mouse_shown(button, h.mods) {
            self.mouse.last_click = if h.dragged {
                None
            } else {
                Some(ClickRecord {
                    button,
                    x: h.x0,
                    y: h.y0,
                    t_down: h.t0,
                    callout: None,
                })
            };
            return;
        }
        if h.dragged {
            self.mouse.last_click = None;
            if button == MouseButton::Right
                && t.saturating_sub(h.t0) <= self.rules.gesture_max_ms
                && dist >= f64::from(self.rules.gesture_min_px)
            {
                let dir = Direction::from_vector(dx, dy);
                self.mouse_callout(button, MouseAction::Gesture, h.mods, Some(dir), t, out);
            } else if h.callout.is_none() {
                self.mouse_callout(button, MouseAction::Drag, h.mods, None, t, out);
            }
            return;
        }
        // A click. Double click: same button within the time and distance.
        let double = self.mouse.last_click.filter(|c| {
            let ddx = f64::from(h.x0 - c.x);
            let ddy = f64::from(h.y0 - c.y);
            c.button == button
                && h.t0.saturating_sub(c.t_down) <= self.rules.double_click_ms
                && (ddx * ddx + ddy * ddy).sqrt() <= f64::from(self.rules.double_click_px)
        });
        if let Some(first) = double {
            self.mouse.last_click = None;
            let is_current = matches!((&self.current, first.callout), (Some(cur), Some(id)) if cur.callout.id == id && cur.callout.count == 1);
            if is_current {
                let chip = mouse_chip(button, MouseAction::DoubleClick, None);
                let mut chips = self.held_mod_chips(h.mods);
                chips.push(chip.clone());
                let cap = self
                    .index
                    .lookup_mouse(&MouseQuery {
                        button,
                        action: MouseAction::DoubleClick,
                        mods: h.mods,
                        direction: None,
                    })
                    .cloned();
                if let Some(cur) = &mut self.current {
                    cur.callout.steps = vec![chips];
                    cur.callout.caption = cap.as_ref().map(|c| c.command.clone());
                    cur.callout.context = cap.and_then(|c| c.context);
                    cur.callout.updated_ms = t;
                    cur.last_ms = t;
                    cur.signature = Some(format!("mouse:{}:{}", h.mods.bits(), chip.key));
                    out.push(DisplayEvent::Callout(cur.callout.clone()));
                    self.stats.callout_updates += 1;
                }
            } else {
                self.mouse_callout(button, MouseAction::DoubleClick, h.mods, None, t, out);
            }
            return;
        }
        let id = self.mouse_callout(button, MouseAction::Click, h.mods, None, t, out);
        self.mouse.last_click = Some(ClickRecord {
            button,
            x: h.x0,
            y: h.y0,
            t_down: h.t0,
            callout: Some(id),
        });
    }

    fn on_move(&mut self, x: i32, y: i32, t: u64, out: &mut Vec<DisplayEvent>) {
        if !self.visible() {
            return;
        }
        if self.monitor.is_some() {
            let (nx, ny) = self.norm(x, y);
            out.push(DisplayEvent::Move {
                x: nx,
                y: ny,
                t_ms: t,
            });
        }
        let threshold = f64::from(self.rules.drag_threshold_px);
        for button in MouseButton::ALL {
            let i = button.index();
            let Some(h) = self.mouse.buttons[i] else {
                continue;
            };
            if h.dragged || !h.revealed {
                continue;
            }
            let dx = f64::from(x - h.x0);
            let dy = f64::from(y - h.y0);
            if (dx * dx + dy * dy).sqrt() <= threshold {
                continue;
            }
            let mut callout = None;
            if button != MouseButton::Right && self.mouse_shown(button, h.mods) {
                callout = Some(self.mouse_callout(button, MouseAction::Drag, h.mods, None, t, out));
            }
            if let Some(b) = self.mouse.buttons[i].as_mut() {
                b.dragged = true;
                b.callout = callout;
            }
        }
    }

    fn on_wheel(&mut self, dx: i32, dy: i32, x: i32, y: i32, t: u64, out: &mut Vec<DisplayEvent>) {
        if !self.visible() {
            return;
        }
        if let Some(tap) = &mut self.tap {
            tap.broken = true;
        }
        let (nx, ny) = self.norm(x, y);
        out.push(DisplayEvent::Wheel {
            dx,
            dy,
            x: nx,
            y: ny,
            t_ms: t,
        });
        if !self.rules.mouse_callouts {
            return;
        }
        let m = &mut self.mouse;
        if t.saturating_sub(m.wheel_last_ms) > self.rules.repeat_window_ms {
            m.acc_x = 0;
            m.acc_y = 0;
        }
        m.wheel_last_ms = t;
        // Reversing direction starts counting afresh.
        if dy != 0 && m.acc_y.signum() == -dy.signum() {
            m.acc_y = 0;
        }
        if dx != 0 && m.acc_x.signum() == -dx.signum() {
            m.acc_x = 0;
        }
        m.acc_y = m.acc_y.saturating_add(dy);
        m.acc_x = m.acc_x.saturating_add(dx);
        let notches_y = m.acc_y / WHEEL_DELTA;
        m.acc_y -= notches_y * WHEEL_DELTA;
        let notches_x = m.acc_x / WHEEL_DELTA;
        m.acc_x -= notches_x * WHEEL_DELTA;
        let mods = self.mods.held();
        if notches_y != 0 {
            let dir = if notches_y > 0 {
                Direction::Up
            } else {
                Direction::Down
            };
            self.mouse_callout_n(
                MouseButton::Middle,
                MouseAction::Wheel,
                mods,
                Some(dir),
                notches_y.unsigned_abs(),
                t,
                out,
            );
        }
        if notches_x != 0 {
            let dir = if notches_x > 0 {
                Direction::Right
            } else {
                Direction::Left
            };
            self.mouse_callout_n(
                MouseButton::Middle,
                MouseAction::Wheel,
                mods,
                Some(dir),
                notches_x.unsigned_abs(),
                t,
                out,
            );
        }
    }

    // ---- resync -------------------------------------------------------

    fn on_resync(
        &mut self,
        held: &[(crate::event::Mod, Side)],
        t: u64,
        out: &mut Vec<DisplayEvent>,
    ) {
        let visible = self.visible();
        let diff = self.mods.resync(held, t);
        for (slot, h) in &diff.released {
            self.stats.released_stale += 1;
            if *slot == ModSlot::AltGr {
                self.fake_ctrl_down = false;
            }
            if h.revealed && visible {
                out.push(self.mod_key_event(*slot, h.phys, false, t));
            }
        }
        for slot in &diff.pressed {
            if visible {
                let phys = keys::modifier_phys(slot.key(), slot.side());
                if let Some(m) = self.mods.get_mut(*slot) {
                    m.revealed = true;
                }
                out.push(self.mod_key_event(*slot, phys, true, t));
            }
        }
        if !self.mods.is_held(ModSlot::CtrlL) && !self.mods.is_held(ModSlot::AltGr) {
            self.fake_ctrl_down = false;
            self.altgr_armed = false;
        }
        if !diff.released.is_empty() || !diff.pressed.is_empty() {
            // A tap group that spans a resync is not a clean tap.
            self.tap = None;
        }
    }
}

fn signature(steps: &[Step]) -> String {
    let mut s = String::new();
    for (i, st) in steps.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&st.mods.bits().to_string());
        s.push(':');
        if let Some(k) = &st.key {
            s.push_str(k);
        }
    }
    s
}

/// The chip for a mouse action: key `MiddleDrag`, label `Middle drag`. Keys
/// always start with the part (`Left`, `Right`, `Middle`, `X1`, `X2`,
/// `Wheel`), then the action, then a direction: `WheelUp`, `LeftDoubleClick`,
/// `RightGestureUpRight`. The renderer reads them in that order.
fn mouse_chip(button: MouseButton, action: MouseAction, direction: Option<Direction>) -> Chip {
    let (key, label) = match action {
        MouseAction::Click => (
            format!("{}Click", button.word()),
            format!("{} click", button.word()),
        ),
        MouseAction::DoubleClick => (
            format!("{}DoubleClick", button.word()),
            format!("{} double click", button.word()),
        ),
        MouseAction::Drag => (
            format!("{}Drag", button.word()),
            format!("{} drag", button.word()),
        ),
        MouseAction::Wheel => {
            let d = direction.unwrap_or(Direction::Up);
            (
                format!("Wheel{}", d.pascal()),
                format!("Wheel {}", d.words()),
            )
        }
        MouseAction::Gesture => {
            let d = direction.unwrap_or(Direction::Up);
            (
                format!("{}Gesture{}", button.word(), d.pascal()),
                format!("Gesture {}", d.words()),
            )
        }
    };
    Chip {
        key,
        label,
        kind: ChipKind::Mouse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_chips() {
        let c = mouse_chip(MouseButton::Middle, MouseAction::Drag, None);
        assert_eq!(
            (c.key.as_str(), c.label.as_str()),
            ("MiddleDrag", "Middle drag")
        );
        let c = mouse_chip(
            MouseButton::Middle,
            MouseAction::Wheel,
            Some(Direction::Down),
        );
        assert_eq!(
            (c.key.as_str(), c.label.as_str()),
            ("WheelDown", "Wheel down")
        );
        let c = mouse_chip(
            MouseButton::Right,
            MouseAction::Gesture,
            Some(Direction::UpRight),
        );
        assert_eq!(
            (c.key.as_str(), c.label.as_str()),
            ("RightGestureUpRight", "Gesture up-right")
        );
        let c = mouse_chip(MouseButton::Left, MouseAction::DoubleClick, None);
        assert_eq!(c.label, "Left double click");
    }

    #[test]
    fn signatures() {
        let s = vec![Step::new(ModSet::EMPTY.with(ModKey::Ctrl), Some("Z"))];
        assert_eq!(signature(&s), "1:Z");
    }
}
