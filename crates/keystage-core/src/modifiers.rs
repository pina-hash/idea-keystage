//! Modifier keys: the display set (`ModSet`) and the left/right tracking the
//! processor keeps from the event stream (`ModifierState`), with resync from
//! `GetAsyncKeyState` and aging of modifiers whose key-up was missed.

use std::fmt;

use crate::event::{Mod, PhysKey, Side};
use crate::keys;

/// A modifier as shown on screen. Unlike [`Mod`] it includes AltGr.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModKey {
    Ctrl,
    Shift,
    Alt,
    AltGr,
    Win,
}

impl ModKey {
    /// Display and matching order (section 4): Ctrl, Shift, Alt, AltGr, Win.
    pub const ORDER: [ModKey; 5] = [
        ModKey::Ctrl,
        ModKey::Shift,
        ModKey::Alt,
        ModKey::AltGr,
        ModKey::Win,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModKey::Ctrl => "Ctrl",
            ModKey::Shift => "Shift",
            ModKey::Alt => "Alt",
            ModKey::AltGr => "AltGr",
            ModKey::Win => "Win",
        }
    }

    /// The canonical name only (`"Ctrl"`, not `"Control"`).
    pub fn from_name(name: &str) -> Option<ModKey> {
        Some(match name {
            "Ctrl" => ModKey::Ctrl,
            "Shift" => ModKey::Shift,
            "Alt" => ModKey::Alt,
            "AltGr" => ModKey::AltGr,
            "Win" => ModKey::Win,
            _ => return None,
        })
    }

    /// Lowercase spellings used in accelerators and documentation.
    pub fn from_alias(lower: &str) -> Option<ModKey> {
        Some(match lower {
            "ctrl" | "control" | "ctl" | "commandorcontrol" | "cmdorctrl" | "commandorctrl"
            | "cmdorcontrol" => ModKey::Ctrl,
            "shift" => ModKey::Shift,
            "alt" | "option" => ModKey::Alt,
            "altgr" | "altgraph" => ModKey::AltGr,
            "win" | "windows" | "super" | "meta" | "cmd" | "command" | "logo" => ModKey::Win,
            _ => return None,
        })
    }

    pub fn bit(self) -> u8 {
        match self {
            ModKey::Ctrl => 1,
            ModKey::Shift => 2,
            ModKey::Alt => 4,
            ModKey::AltGr => 8,
            ModKey::Win => 16,
        }
    }

    /// The wire `modifier` value of a key event for this modifier. AltGr is
    /// the right Alt key, so the keyboard visual hears `alt`.
    pub fn wire(self) -> Mod {
        match self {
            ModKey::Ctrl => Mod::Ctrl,
            ModKey::Shift => Mod::Shift,
            ModKey::Alt | ModKey::AltGr => Mod::Alt,
            ModKey::Win => Mod::Win,
        }
    }

    pub fn from_mod(m: Mod) -> ModKey {
        match m {
            Mod::Ctrl => ModKey::Ctrl,
            Mod::Shift => ModKey::Shift,
            Mod::Alt => ModKey::Alt,
            Mod::Win => ModKey::Win,
        }
    }
}

/// A set of display modifiers.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModSet(u8);

impl ModSet {
    pub const EMPTY: ModSet = ModSet(0);

    pub fn from_bits(bits: u8) -> ModSet {
        ModSet(bits & 0x1F)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn contains(self, m: ModKey) -> bool {
        self.0 & m.bit() != 0
    }

    pub fn insert(&mut self, m: ModKey) {
        self.0 |= m.bit();
    }

    pub fn remove(&mut self, m: ModKey) {
        self.0 &= !m.bit();
    }

    pub fn with(mut self, m: ModKey) -> ModSet {
        self.insert(m);
        self
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// Ctrl, Alt or Win: modifiers that turn a key into a command. Shift and
    /// AltGr alone still type characters.
    pub fn has_command(self) -> bool {
        self.0 & (ModKey::Ctrl.bit() | ModKey::Alt.bit() | ModKey::Win.bit()) != 0
    }

    /// Iterates in display order.
    pub fn iter(self) -> impl Iterator<Item = ModKey> {
        ModKey::ORDER.into_iter().filter(move |m| self.contains(*m))
    }

    /// Builds a set from canonical names; returns `None` if a name is not a
    /// modifier.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<ModSet> {
        let mut s = ModSet::EMPTY;
        for n in names {
            s.insert(ModKey::from_name(n)?);
        }
        Some(s)
    }
}

impl fmt::Debug for ModSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.iter().map(ModKey::name))
            .finish()
    }
}

/// One tracked physical modifier key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModSlot {
    CtrlL,
    CtrlR,
    ShiftL,
    ShiftR,
    AltL,
    AltR,
    WinL,
    WinR,
    /// The right Alt key acting as AltGr.
    AltGr,
}

impl ModSlot {
    pub const ALL: [ModSlot; 9] = [
        ModSlot::CtrlL,
        ModSlot::CtrlR,
        ModSlot::ShiftL,
        ModSlot::ShiftR,
        ModSlot::AltL,
        ModSlot::AltR,
        ModSlot::WinL,
        ModSlot::WinR,
        ModSlot::AltGr,
    ];

    pub fn of(m: ModKey, side: Side) -> ModSlot {
        match (m, side) {
            (ModKey::Ctrl, Side::Left) => ModSlot::CtrlL,
            (ModKey::Ctrl, Side::Right) => ModSlot::CtrlR,
            (ModKey::Shift, Side::Left) => ModSlot::ShiftL,
            (ModKey::Shift, Side::Right) => ModSlot::ShiftR,
            (ModKey::Alt, Side::Left) => ModSlot::AltL,
            (ModKey::Alt, Side::Right) => ModSlot::AltR,
            (ModKey::Win, Side::Left) => ModSlot::WinL,
            (ModKey::Win, Side::Right) => ModSlot::WinR,
            (ModKey::AltGr, _) => ModSlot::AltGr,
        }
    }

    pub fn key(self) -> ModKey {
        match self {
            ModSlot::CtrlL | ModSlot::CtrlR => ModKey::Ctrl,
            ModSlot::ShiftL | ModSlot::ShiftR => ModKey::Shift,
            ModSlot::AltL | ModSlot::AltR => ModKey::Alt,
            ModSlot::WinL | ModSlot::WinR => ModKey::Win,
            ModSlot::AltGr => ModKey::AltGr,
        }
    }

    pub fn side(self) -> Side {
        match self {
            ModSlot::CtrlL | ModSlot::ShiftL | ModSlot::AltL | ModSlot::WinL => Side::Left,
            _ => Side::Right,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A modifier key that is down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldMod {
    pub phys: PhysKey,
    /// When it went down.
    pub since_ms: u64,
    /// The last time something proved it is still down: its own key-down or
    /// auto-repeat, or a resync that listed it.
    pub evidence_ms: u64,
    /// A key-down event was sent for it, so a key-up must follow.
    pub revealed: bool,
}

/// What a resync changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResyncDiff {
    /// Modifiers the core thought were down but are not.
    pub released: Vec<(ModSlot, HeldMod)>,
    /// Modifiers that are down but the core missed going down.
    pub pressed: Vec<ModSlot>,
}

/// Left and right modifier tracking.
#[derive(Clone, Debug, Default)]
pub struct ModifierState {
    slots: [Option<HeldMod>; 9],
}

impl ModifierState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, slot: ModSlot) -> Option<&HeldMod> {
        self.slots[slot.index()].as_ref()
    }

    pub fn get_mut(&mut self, slot: ModSlot) -> Option<&mut HeldMod> {
        self.slots[slot.index()].as_mut()
    }

    pub fn is_held(&self, slot: ModSlot) -> bool {
        self.slots[slot.index()].is_some()
    }

    pub fn press(&mut self, slot: ModSlot, phys: PhysKey, t_ms: u64, revealed: bool) {
        self.slots[slot.index()] = Some(HeldMod {
            phys,
            since_ms: t_ms,
            evidence_ms: t_ms,
            revealed,
        });
    }

    /// Records evidence that a held modifier is still down.
    pub fn refresh(&mut self, slot: ModSlot, t_ms: u64) {
        if let Some(h) = self.slots[slot.index()].as_mut() {
            h.evidence_ms = h.evidence_ms.max(t_ms);
        }
    }

    pub fn release(&mut self, slot: ModSlot) -> Option<HeldMod> {
        self.slots[slot.index()].take()
    }

    /// The display set of held modifiers.
    pub fn held(&self) -> ModSet {
        let mut s = ModSet::EMPTY;
        for slot in ModSlot::ALL {
            if self.is_held(slot) {
                s.insert(slot.key());
            }
        }
        s
    }

    pub fn any_held(&self) -> bool {
        self.slots.iter().any(Option::is_some)
    }

    /// The side of a held modifier, left first.
    pub fn side_of(&self, m: ModKey) -> Option<Side> {
        [Side::Left, Side::Right]
            .into_iter()
            .find(|s| self.is_held(ModSlot::of(m, *s)))
    }

    /// Every held slot with its state.
    pub fn held_slots(&self) -> Vec<(ModSlot, HeldMod)> {
        ModSlot::ALL
            .into_iter()
            .filter_map(|s| self.get(s).map(|h| (s, *h)))
            .collect()
    }

    /// Marks every held modifier as not revealed (no key-up will be sent).
    pub fn conceal_all(&mut self) {
        for h in self.slots.iter_mut().flatten() {
            h.revealed = false;
        }
    }

    /// Forgets every held modifier and returns them.
    pub fn clear(&mut self) -> Vec<(ModSlot, HeldMod)> {
        let out = self.held_slots();
        self.slots = Default::default();
        out
    }

    /// The oldest evidence among held modifiers.
    pub fn oldest_evidence(&self) -> Option<u64> {
        self.slots.iter().flatten().map(|h| h.evidence_ms).min()
    }

    /// Releases modifiers with no evidence for more than `stale_ms`.
    pub fn age(&mut self, now_ms: u64, stale_ms: u64) -> Vec<(ModSlot, HeldMod)> {
        let mut out = Vec::new();
        for slot in ModSlot::ALL {
            if let Some(h) = self.get(slot) {
                if now_ms.saturating_sub(h.evidence_ms) > stale_ms {
                    out.push((slot, *h));
                    self.slots[slot.index()] = None;
                }
            }
        }
        out
    }

    /// Replaces the tracked state with what the system reports. The AltGr
    /// slot is kept while the right Alt key is reported down, and a reported
    /// left Ctrl is then taken to be AltGr's fake Ctrl, not a real one.
    pub fn resync(&mut self, held: &[(Mod, Side)], t_ms: u64) -> ResyncDiff {
        let mut wanted = [false; 9];
        let altgr_down = self.is_held(ModSlot::AltGr);
        for (m, side) in held {
            let mut slot = ModSlot::of(ModKey::from_mod(*m), *side);
            if altgr_down && slot == ModSlot::AltR {
                slot = ModSlot::AltGr;
            }
            if altgr_down && slot == ModSlot::CtrlL && !self.is_held(ModSlot::CtrlL) {
                continue;
            }
            wanted[slot.index()] = true;
        }
        let mut diff = ResyncDiff::default();
        for slot in ModSlot::ALL {
            let i = slot.index();
            match (self.slots[i], wanted[i]) {
                (Some(h), false) => {
                    diff.released.push((slot, h));
                    self.slots[i] = None;
                }
                (Some(_), true) => self.refresh(slot, t_ms),
                (None, true) => {
                    let phys = keys::modifier_phys(slot.key(), slot.side());
                    self.press(slot, phys, t_ms, false);
                    diff.pressed.push(slot);
                }
                (None, false) => {}
            }
        }
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modset_basics() {
        let mut s = ModSet::EMPTY;
        assert!(s.is_empty());
        s.insert(ModKey::Win);
        s.insert(ModKey::Ctrl);
        s.insert(ModKey::Shift);
        assert_eq!(
            s.iter().collect::<Vec<_>>(),
            vec![ModKey::Ctrl, ModKey::Shift, ModKey::Win]
        );
        assert_eq!(s.len(), 3);
        assert!(s.has_command());
        s.remove(ModKey::Ctrl);
        s.remove(ModKey::Win);
        assert!(!s.has_command(), "Shift alone types");
        assert!(
            !ModSet::EMPTY.with(ModKey::AltGr).has_command(),
            "AltGr alone types"
        );
        assert_eq!(
            ModSet::from_names(["Alt", "Ctrl"]),
            Some(ModSet::EMPTY.with(ModKey::Ctrl).with(ModKey::Alt))
        );
        assert_eq!(ModSet::from_names(["Alt", "A"]), None);
        assert_eq!(
            format!("{:?}", ModSet::EMPTY.with(ModKey::Alt).with(ModKey::Ctrl)),
            "[\"Ctrl\", \"Alt\"]"
        );
        assert_eq!(ModSet::from_bits(0xFF).bits(), 0x1F);
    }

    #[test]
    fn aliases() {
        assert_eq!(ModKey::from_alias("commandorcontrol"), Some(ModKey::Ctrl));
        assert_eq!(ModKey::from_alias("super"), Some(ModKey::Win));
        assert_eq!(ModKey::from_alias("option"), Some(ModKey::Alt));
        assert_eq!(ModKey::from_alias("menu"), None, "Menu is a key, not Alt");
        assert_eq!(ModKey::AltGr.wire(), Mod::Alt);
    }

    #[test]
    fn left_and_right_are_separate() {
        let mut st = ModifierState::new();
        st.press(
            ModSlot::CtrlL,
            keys::modifier_phys(ModKey::Ctrl, Side::Left),
            0,
            true,
        );
        st.press(
            ModSlot::CtrlR,
            keys::modifier_phys(ModKey::Ctrl, Side::Right),
            5,
            true,
        );
        assert_eq!(st.held(), ModSet::EMPTY.with(ModKey::Ctrl));
        st.release(ModSlot::CtrlL);
        assert_eq!(
            st.held(),
            ModSet::EMPTY.with(ModKey::Ctrl),
            "right Ctrl still down"
        );
        assert_eq!(st.side_of(ModKey::Ctrl), Some(Side::Right));
        st.release(ModSlot::CtrlR);
        assert!(st.held().is_empty());
    }

    #[test]
    fn resync_releases_and_presses() {
        let mut st = ModifierState::new();
        st.press(
            ModSlot::CtrlL,
            keys::modifier_phys(ModKey::Ctrl, Side::Left),
            0,
            true,
        );
        st.press(
            ModSlot::WinL,
            keys::modifier_phys(ModKey::Win, Side::Left),
            0,
            true,
        );
        let d = st.resync(&[(Mod::Shift, Side::Right), (Mod::Ctrl, Side::Left)], 100);
        assert_eq!(d.released.len(), 1);
        assert_eq!(d.released[0].0, ModSlot::WinL);
        assert_eq!(d.pressed, vec![ModSlot::ShiftR]);
        assert_eq!(st.get(ModSlot::CtrlL).unwrap().evidence_ms, 100);
        assert_eq!(
            st.held(),
            ModSet::EMPTY.with(ModKey::Ctrl).with(ModKey::Shift)
        );
    }

    #[test]
    fn resync_keeps_altgr() {
        let mut st = ModifierState::new();
        st.press(
            ModSlot::AltGr,
            keys::modifier_phys(ModKey::AltGr, Side::Right),
            0,
            true,
        );
        let d = st.resync(&[(Mod::Ctrl, Side::Left), (Mod::Alt, Side::Right)], 50);
        assert!(d.released.is_empty() && d.pressed.is_empty(), "{d:?}");
        assert_eq!(st.held(), ModSet::EMPTY.with(ModKey::AltGr));
        let d = st.resync(&[], 60);
        assert_eq!(d.released.len(), 1);
        assert!(st.held().is_empty());
    }

    #[test]
    fn aging() {
        let mut st = ModifierState::new();
        st.press(
            ModSlot::ShiftL,
            keys::modifier_phys(ModKey::Shift, Side::Left),
            0,
            true,
        );
        st.press(
            ModSlot::AltL,
            keys::modifier_phys(ModKey::Alt, Side::Left),
            500,
            true,
        );
        assert_eq!(st.oldest_evidence(), Some(0));
        assert!(st.age(1000, 1000).is_empty());
        let aged = st.age(1001, 1000);
        assert_eq!(aged.len(), 1);
        assert_eq!(aged[0].0, ModSlot::ShiftL);
        st.refresh(ModSlot::AltL, 1400);
        assert!(st.age(2000, 1000).is_empty());
        assert_eq!(st.age(2401, 1000).len(), 1);
        assert!(!st.any_held());
    }
}
