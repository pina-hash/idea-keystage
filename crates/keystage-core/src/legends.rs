//! Display legends for character keys (ARCHITECTURE.md section 3).
//!
//! The platform layer implements [`LegendProvider`] from the foreground
//! window's keyboard layout (`ToUnicodeEx` with flag 0x4). Tests and the demo
//! use [`UsLegends`], the US layout. Profile matching never uses legends; it
//! uses canonical names, which come from the VK.

use std::collections::BTreeMap;

use crate::event::PhysKey;
use crate::keys::{self, vk, KeyClass};
use crate::protocol::Legends;

/// The modifier state that decides which character a key types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextMods {
    pub shift: bool,
    pub altgr: bool,
    pub caps_lock: bool,
}

/// Keycap legends and typed characters for the active keyboard layout.
pub trait LegendProvider {
    /// The keycap legend of a key (uppercase for letters), or `None` if the
    /// layout has no character for it.
    fn legend(&self, vk: u16, phys: PhysKey) -> Option<String>;

    /// The text the key types with these modifiers, for text runs. The
    /// default derives it from [`LegendProvider::legend`]: letters follow
    /// Shift and Caps Lock, other keys type their legend.
    fn text(&self, vk: u16, phys: PhysKey, mods: TextMods) -> Option<String> {
        let legend = self.legend(vk, phys)?;
        let mut chars = legend.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_alphabetic() => {
                if mods.shift ^ mods.caps_lock {
                    Some(c.to_uppercase().collect())
                } else {
                    Some(c.to_lowercase().collect())
                }
            }
            _ => Some(legend),
        }
    }
}

/// A provider with no legends: chips fall back to canonical names.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLegends;

impl LegendProvider for NoLegends {
    fn legend(&self, _vk: u16, _phys: PhysKey) -> Option<String> {
        None
    }
}

/// The US English layout (00000409).
#[derive(Clone, Copy, Debug, Default)]
pub struct UsLegends;

impl UsLegends {
    pub const LAYOUT_ID: &'static str = "00000409";

    fn pair(code: u16) -> Option<(char, char)> {
        Some(match code {
            vk::KEY_0..=vk::KEY_9 => {
                let d = (b'0' + (code - vk::KEY_0) as u8) as char;
                let shifted =
                    [')', '!', '@', '#', '$', '%', '^', '&', '*', '('][(code - vk::KEY_0) as usize];
                (d, shifted)
            }
            vk::KEY_A..=vk::KEY_Z => {
                let c = (b'a' + (code - vk::KEY_A) as u8) as char;
                (c, c.to_ascii_uppercase())
            }
            vk::SPACE => (' ', ' '),
            vk::OEM_1 => (';', ':'),
            vk::OEM_PLUS => ('=', '+'),
            vk::OEM_COMMA => (',', '<'),
            vk::OEM_MINUS => ('-', '_'),
            vk::OEM_PERIOD => ('.', '>'),
            vk::OEM_2 => ('/', '?'),
            vk::OEM_3 => ('`', '~'),
            vk::OEM_4 => ('[', '{'),
            vk::OEM_5 | vk::OEM_102 => ('\\', '|'),
            vk::OEM_6 => (']', '}'),
            vk::OEM_7 => ('\'', '"'),
            vk::NUMPAD0..=vk::NUMPAD9 => {
                let d = (b'0' + (code - vk::NUMPAD0) as u8) as char;
                (d, d)
            }
            vk::MULTIPLY => ('*', '*'),
            vk::ADD => ('+', '+'),
            vk::SUBTRACT => ('-', '-'),
            vk::DECIMAL => ('.', '.'),
            vk::DIVIDE => ('/', '/'),
            _ => return None,
        })
    }
}

impl LegendProvider for UsLegends {
    fn legend(&self, code: u16, _phys: PhysKey) -> Option<String> {
        if code == vk::SPACE {
            return None;
        }
        let (base, _) = Self::pair(code)?;
        Some(base.to_ascii_uppercase().to_string())
    }

    fn text(&self, code: u16, _phys: PhysKey, mods: TextMods) -> Option<String> {
        if mods.altgr {
            // The US layout has no AltGr characters.
            return None;
        }
        let (base, shifted) = Self::pair(code)?;
        let c = if base.is_ascii_alphabetic() {
            if mods.shift ^ mods.caps_lock {
                shifted
            } else {
                base
            }
        } else if mods.shift {
            shifted
        } else {
            base
        };
        Some(c.to_string())
    }
}

/// The display label of a key: the layout legend for character keys, the
/// word label for everything else.
pub fn chip_label(provider: &dyn LegendProvider, code: u16, phys: PhysKey, name: &str) -> String {
    let info = keys::classify(code, phys);
    if matches!(info.class, KeyClass::Character) && name != "Space" && !name.starts_with("Num") {
        if let Some(l) = provider.legend(code, phys) {
            let trimmed = l.trim();
            if !trimmed.is_empty() && !trimmed.chars().any(char::is_control) {
                return trimmed.to_string();
            }
        }
    }
    keys::word_label(name).to_string()
}

/// Builds a `legends` message from `(physical key, VK)` pairs, typically
/// [`keys::CHARACTER_SCANS`] mapped through the active layout.
pub fn build_legends(
    layout: &str,
    keys_iter: impl IntoIterator<Item = (PhysKey, u16)>,
    provider: &dyn LegendProvider,
) -> Legends {
    let mut map = BTreeMap::new();
    for (phys, code) in keys_iter {
        if let Some(l) = provider.legend(code, phys) {
            let trimmed = l.trim();
            if !trimmed.is_empty() && !trimmed.chars().any(char::is_control) {
                map.insert(phys.wire_id(), trimmed.to_string());
            }
        }
    }
    Legends {
        layout: layout.to_string(),
        legends: map,
    }
}

/// The US set 1 scan code of each character VK, for building US legends in
/// tests and the demo without a Windows layout.
pub fn us_scan_to_vk(scan: u16) -> Option<u16> {
    Some(match scan {
        0x02..=0x0A => vk::KEY_0 + 1 + (scan - 0x02),
        0x0B => vk::KEY_0,
        0x0C => vk::OEM_MINUS,
        0x0D => vk::OEM_PLUS,
        0x10 => 0x51,
        0x11 => 0x57,
        0x12 => 0x45,
        0x13 => 0x52,
        0x14 => 0x54,
        0x15 => 0x59,
        0x16 => 0x55,
        0x17 => 0x49,
        0x18 => 0x4F,
        0x19 => 0x50,
        0x1A => vk::OEM_4,
        0x1B => vk::OEM_6,
        0x1E => 0x41,
        0x1F => 0x53,
        0x20 => 0x44,
        0x21 => 0x46,
        0x22 => 0x47,
        0x23 => 0x48,
        0x24 => 0x4A,
        0x25 => 0x4B,
        0x26 => 0x4C,
        0x27 => vk::OEM_1,
        0x28 => vk::OEM_7,
        0x29 => vk::OEM_3,
        0x2B => vk::OEM_5,
        0x2C => 0x5A,
        0x2D => 0x58,
        0x2E => 0x43,
        0x2F => 0x56,
        0x30 => 0x42,
        0x31 => 0x4E,
        0x32 => 0x4D,
        0x33 => vk::OEM_COMMA,
        0x34 => vk::OEM_PERIOD,
        0x35 => vk::OEM_2,
        0x39 => vk::SPACE,
        0x56 => vk::OEM_102,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(scan: u16) -> PhysKey {
        PhysKey::new(scan, false)
    }

    #[test]
    fn us_legends_and_text() {
        let us = UsLegends;
        assert_eq!(us.legend(0x41, p(0x1E)).as_deref(), Some("A"));
        assert_eq!(us.legend(vk::OEM_1, p(0x27)).as_deref(), Some(";"));
        assert_eq!(us.legend(vk::SPACE, p(0x39)), None);
        assert_eq!(us.legend(vk::F1, p(0x3B)), None);
        let plain = TextMods::default();
        let shift = TextMods {
            shift: true,
            ..plain
        };
        let caps = TextMods {
            caps_lock: true,
            ..plain
        };
        assert_eq!(us.text(0x41, p(0x1E), plain).as_deref(), Some("a"));
        assert_eq!(us.text(0x41, p(0x1E), shift).as_deref(), Some("A"));
        assert_eq!(us.text(0x41, p(0x1E), caps).as_deref(), Some("A"));
        assert_eq!(
            us.text(
                0x41,
                p(0x1E),
                TextMods {
                    shift: true,
                    caps_lock: true,
                    ..plain
                }
            )
            .as_deref(),
            Some("a")
        );
        assert_eq!(us.text(0x31, p(0x02), shift).as_deref(), Some("!"));
        assert_eq!(
            us.text(0x31, p(0x02), caps).as_deref(),
            Some("1"),
            "Caps Lock does not shift digits"
        );
        assert_eq!(us.text(vk::SPACE, p(0x39), plain).as_deref(), Some(" "));
        assert_eq!(us.text(vk::DECIMAL, p(0x53), plain).as_deref(), Some("."));
        assert_eq!(us.text(vk::NUMPAD7, p(0x47), plain).as_deref(), Some("7"));
        assert_eq!(
            us.text(
                0x41,
                p(0x1E),
                TextMods {
                    altgr: true,
                    ..plain
                }
            ),
            None
        );
    }

    struct Fixed;
    impl LegendProvider for Fixed {
        fn legend(&self, code: u16, _phys: PhysKey) -> Option<String> {
            match code {
                0x41 => Some("Q".into()),
                vk::OEM_1 => Some("Ü".into()),
                vk::OEM_7 => Some("\u{7}".into()),
                _ => None,
            }
        }
    }

    #[test]
    fn default_text_follows_legend() {
        let f = Fixed;
        let plain = TextMods::default();
        assert_eq!(f.text(0x41, p(0x1E), plain).as_deref(), Some("q"));
        assert_eq!(
            f.text(
                vk::OEM_1,
                p(0x1A),
                TextMods {
                    shift: true,
                    ..plain
                }
            )
            .as_deref(),
            Some("Ü")
        );
        assert_eq!(f.text(vk::OEM_1, p(0x1A), plain).as_deref(), Some("ü"));
    }

    #[test]
    fn chip_labels() {
        let f = Fixed;
        assert_eq!(
            chip_label(&f, 0x41, p(0x1E), "A"),
            "Q",
            "layout legend wins for display"
        );
        assert_eq!(
            chip_label(&f, vk::OEM_7, p(0x28), "'"),
            "'",
            "control characters are refused"
        );
        assert_eq!(
            chip_label(&f, vk::NEXT, PhysKey::new(0x51, true), "PageDown"),
            "Page Down"
        );
        assert_eq!(chip_label(&NoLegends, 0x41, p(0x1E), "A"), "A");
        assert_eq!(chip_label(&UsLegends, vk::SPACE, p(0x39), "Space"), "Space");
        assert_eq!(
            chip_label(&UsLegends, vk::NUMPAD1, p(0x4F), "Num1"),
            "Num 1"
        );
    }

    #[test]
    fn us_legend_message() {
        let pairs = keys::CHARACTER_SCANS
            .iter()
            .filter_map(|s| us_scan_to_vk(*s).map(|v| (p(*s), v)));
        let l = build_legends(UsLegends::LAYOUT_ID, pairs, &UsLegends);
        assert_eq!(l.layout, "00000409");
        assert_eq!(l.legends.get("sc:1E").map(String::as_str), Some("A"));
        assert_eq!(l.legends.get("sc:27").map(String::as_str), Some(";"));
        assert_eq!(l.legends.get("sc:56").map(String::as_str), Some("\\"));
        assert!(!l.legends.contains_key("sc:39"), "Space has no legend");
        assert_eq!(l.legends.len(), keys::CHARACTER_SCANS.len() - 1);
        for s in keys::CHARACTER_SCANS {
            assert!(us_scan_to_vk(*s).is_some(), "scan {s:#04x}");
        }
    }
}
