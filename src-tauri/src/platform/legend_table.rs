//! Keycap legends and typed characters for one keyboard layout.
//!
//! [`LegendTable`] implements [`keystage_core::LegendProvider`]. On Windows
//! it is filled once per layout change by `platform/windows/layout.rs`
//! (`MapVirtualKeyExW` plus `ToUnicodeEx` with flag 0x4); elsewhere, and in
//! tests, [`LegendTable::us`] builds the US layout from the core's
//! [`UsLegends`]. Lookups are plain map reads, so the pipeline thread can
//! call them for every key without touching the operating system.

use std::collections::HashMap;

use keystage_core::keys::{self, vk};
use keystage_core::legends::{build_legends, us_scan_to_vk};
use keystage_core::{LegendProvider, Legends, PhysKey, TextMods, UsLegends};

/// Number of modifier states a [`KeyEntry`] stores typed text for: every
/// combination of Shift, AltGr and Caps Lock.
pub const TEXT_STATES: usize = 8;

/// Index into [`KeyEntry::text`]: bit 0 Shift, bit 1 AltGr, bit 2 Caps Lock.
pub fn text_index(mods: TextMods) -> usize {
    usize::from(mods.shift) | (usize::from(mods.altgr) << 1) | (usize::from(mods.caps_lock) << 2)
}

/// The modifier state for an index of [`KeyEntry::text`].
pub fn mods_for_index(i: usize) -> TextMods {
    TextMods {
        shift: i & 1 != 0,
        altgr: i & 2 != 0,
        caps_lock: i & 4 != 0,
    }
}

/// The 256-byte keyboard state `ToUnicodeEx` reads for a modifier state.
/// Shift sets `VK_SHIFT` and `VK_LSHIFT`; AltGr sets Ctrl and Alt (left Ctrl
/// plus right Alt, which is what Windows reports for AltGr); Caps Lock sets
/// the toggle bit of `VK_CAPITAL` ("only the toggle bit of the CAPS LOCK key
/// is relevant", ToUnicodeEx).
pub fn keyboard_state(mods: TextMods) -> [u8; 256] {
    const DOWN: u8 = 0x80;
    let mut ks = [0u8; 256];
    let mut set = |code: u16, v: u8| {
        if let Some(slot) = ks.get_mut(usize::from(code)) {
            *slot |= v;
        }
    };
    if mods.shift {
        set(vk::SHIFT, DOWN);
        set(vk::LSHIFT, DOWN);
    }
    if mods.altgr {
        set(vk::CONTROL, DOWN);
        set(vk::LCONTROL, DOWN);
        set(vk::MENU, DOWN);
        set(vk::RMENU, DOWN);
    }
    if mods.caps_lock {
        set(vk::CAPITAL, 0x01);
    }
    ks
}

/// What `ToUnicodeEx` produced for one key in one modifier state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Translation {
    /// No character (return value 0, or only control characters).
    #[default]
    None,
    /// One or more characters (return value > 0).
    Chars(String),
    /// A dead key; the string is the spacing form of the accent the
    /// function writes (return value < 0).
    Dead(String),
}

impl Translation {
    /// Decodes a `ToUnicodeEx` return value and buffer. Only the number of
    /// UTF-16 units the return value names are read ("any extra characters
    /// are invalid and should be ignored"); surrogate pairs are kept; a
    /// result containing control characters (Ctrl combinations) is `None`.
    pub fn decode(ret: i32, buf: &[u16]) -> Translation {
        if ret == 0 {
            return Translation::None;
        }
        let n = (ret.unsigned_abs() as usize).min(buf.len());
        let units = buf.get(..n).unwrap_or(&[]);
        let s: String = char::decode_utf16(units.iter().copied())
            .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
        if s.is_empty() || s.chars().any(char::is_control) {
            return Translation::None;
        }
        if ret < 0 {
            Translation::Dead(s)
        } else {
            Translation::Chars(s)
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Translation::None => None,
            Translation::Chars(s) | Translation::Dead(s) => Some(s),
        }
    }

    pub fn is_dead(&self) -> bool {
        matches!(self, Translation::Dead(_))
    }

    /// The text this translation types. A dead key types its spacing
    /// accent: composing it with the next key would need `ToUnicodeEx`
    /// without flag 0x4, which changes the keyboard state the user is
    /// typing with.
    pub fn into_text(self) -> Option<String> {
        match self {
            Translation::None => None,
            Translation::Chars(s) | Translation::Dead(s) => Some(s),
        }
    }
}

/// The keycap legend from the unshifted and shifted characters: a letter is
/// shown in its uppercase form when Shift types exactly that form (`a`/`A`
/// gives `A`, but `ß`/`?` stays `ß`); everything else shows the unshifted
/// character. Whitespace-only results have no legend.
pub fn keycap_legend(base: &Translation, shifted: &Translation) -> Option<String> {
    let b = base.as_str()?.trim();
    if b.is_empty() {
        return None;
    }
    let mut chars = b.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_alphabetic() && c.is_lowercase() {
            let upper: String = c.to_uppercase().collect();
            if shifted.as_str() == Some(upper.as_str()) {
                return Some(upper);
            }
        }
    }
    Some(b.to_string())
}

/// The Windows keyboard layout id (KLID) for an input locale handle (HKL).
///
/// The low word of an HKL is the language, the high word the layout
/// ("The low word contains a Language Identifier for the input language and
/// the high word contains a device handle to the physical layout of the
/// keyboard", GetKeyboardLayout). For standard layouts the high word is the
/// low word of the KLID (`0x04070409` is US English input with the German
/// layout, KLID `00000407`). Layout variants (`0xFxxx`) and IMEs (`0xExxx`)
/// have no KLID in the handle; their id is the handle itself in hex.
pub fn klid_from_hkl(hkl: usize) -> String {
    let lang = hkl & 0xFFFF;
    let device = (hkl >> 16) & 0xFFFF;
    if device >= 0xE000 {
        format!("{:08X}", hkl & 0xFFFF_FFFF)
    } else if device == 0 {
        format!("0000{lang:04X}")
    } else {
        format!("0000{device:04X}")
    }
}

/// One key of a [`LegendTable`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEntry {
    pub phys: PhysKey,
    /// The virtual key this physical key produces in the layout.
    pub vk: u16,
    /// True for the keys of the keyboard visual's character block
    /// ([`keys::CHARACTER_SCANS`]); they carry a keycap legend. Numpad
    /// entries are text-only.
    pub keycap: bool,
    pub legend: Option<String>,
    /// The unshifted key is a dead key (accent).
    pub dead: bool,
    /// Typed text per modifier state, indexed by [`text_index`].
    pub text: [Option<String>; TEXT_STATES],
}

/// The numpad keys that type characters with Num Lock on, with the US scan
/// code each sits on (Keyboard Input Overview scan code table). Their text
/// comes from the layout (VK_DECIMAL types `,` on many layouts).
pub const NUMPAD_TEXT_KEYS: [(u16, u16, bool); 15] = [
    (vk::NUMPAD0, 0x52, false),
    (vk::NUMPAD1, 0x4F, false),
    (vk::NUMPAD2, 0x50, false),
    (vk::NUMPAD3, 0x51, false),
    (vk::NUMPAD4, 0x4B, false),
    (vk::NUMPAD5, 0x4C, false),
    (vk::NUMPAD6, 0x4D, false),
    (vk::NUMPAD7, 0x47, false),
    (vk::NUMPAD8, 0x48, false),
    (vk::NUMPAD9, 0x49, false),
    (vk::DECIMAL, 0x53, false),
    (vk::ADD, 0x4E, false),
    (vk::SUBTRACT, 0x4A, false),
    (vk::MULTIPLY, 0x37, false),
    (vk::DIVIDE, 0x35, true),
];

/// Legends and typed text for one layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegendTable {
    layout: String,
    entries: Vec<KeyEntry>,
    by_phys: HashMap<PhysKey, usize>,
    by_vk: HashMap<u16, usize>,
}

impl LegendTable {
    /// Builds the lookup maps. Keycap entries are found by physical key;
    /// every entry is also found by virtual key (the first entry for a VK
    /// wins).
    pub fn new(layout: impl Into<String>, entries: Vec<KeyEntry>) -> LegendTable {
        let mut by_phys = HashMap::new();
        let mut by_vk = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            if e.keycap {
                by_phys.entry(e.phys).or_insert(i);
            }
            if e.vk != 0 {
                by_vk.entry(e.vk).or_insert(i);
            }
        }
        LegendTable {
            layout: layout.into(),
            entries,
            by_phys,
            by_vk,
        }
    }

    /// The US English layout (`00000409`), from the core's [`UsLegends`].
    pub fn us() -> LegendTable {
        let us = UsLegends;
        let mut entries = Vec::new();
        for &scan in keys::CHARACTER_SCANS {
            let Some(code) = us_scan_to_vk(scan) else {
                continue;
            };
            let phys = PhysKey::new(scan, false);
            entries.push(KeyEntry {
                phys,
                vk: code,
                keycap: true,
                legend: us.legend(code, phys),
                dead: false,
                text: std::array::from_fn(|i| us.text(code, phys, mods_for_index(i))),
            });
        }
        for (code, scan, extended) in NUMPAD_TEXT_KEYS {
            let phys = PhysKey::new(scan, extended);
            entries.push(KeyEntry {
                phys,
                vk: code,
                keycap: false,
                legend: None,
                dead: false,
                text: std::array::from_fn(|i| us.text(code, phys, mods_for_index(i))),
            });
        }
        LegendTable::new(UsLegends::LAYOUT_ID, entries)
    }

    /// The layout id carried in the `legends` wire message.
    pub fn layout(&self) -> &str {
        &self.layout
    }

    pub fn entries(&self) -> &[KeyEntry] {
        &self.entries
    }

    /// The entry for a key: by physical key when the layout puts this VK
    /// there, otherwise by virtual key (a stale scan code, or a numpad key).
    pub fn entry(&self, code: u16, phys: PhysKey) -> Option<&KeyEntry> {
        if let Some(e) = self.by_phys.get(&phys).and_then(|&i| self.entries.get(i)) {
            if e.vk == code {
                return Some(e);
            }
        }
        self.by_vk.get(&code).and_then(|&i| self.entries.get(i))
    }

    /// True when the key is a dead key in this layout.
    pub fn is_dead(&self, code: u16, phys: PhysKey) -> bool {
        self.entry(code, phys).is_some_and(|e| e.dead)
    }

    /// The `legends` wire message: every keycap entry with a legend, keyed
    /// by physical wire id (`"sc:1E": "A"`).
    pub fn wire(&self) -> Legends {
        let pairs = self
            .entries
            .iter()
            .filter(|e| e.keycap)
            .map(|e| (e.phys, e.vk));
        build_legends(&self.layout, pairs, self)
    }
}

impl LegendProvider for LegendTable {
    fn legend(&self, code: u16, phys: PhysKey) -> Option<String> {
        let e = self.entry(code, phys)?;
        if e.keycap {
            e.legend.clone()
        } else {
            None
        }
    }

    fn text(&self, code: u16, phys: PhysKey, mods: TextMods) -> Option<String> {
        self.entry(code, phys)?
            .text
            .get(text_index(mods))
            .cloned()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(scan: u16) -> PhysKey {
        PhysKey::new(scan, false)
    }

    fn chars(s: &str) -> Translation {
        Translation::Chars(s.into())
    }

    #[test]
    fn text_index_round_trips() {
        for i in 0..TEXT_STATES {
            assert_eq!(text_index(mods_for_index(i)), i);
        }
        assert_eq!(text_index(TextMods::default()), 0);
        assert_eq!(
            text_index(TextMods {
                shift: true,
                altgr: false,
                caps_lock: true
            }),
            5
        );
    }

    #[test]
    fn keyboard_state_bits() {
        let plain = keyboard_state(TextMods::default());
        assert!(plain.iter().all(|&b| b == 0));
        let all = keyboard_state(mods_for_index(7));
        for code in [
            vk::SHIFT,
            vk::LSHIFT,
            vk::CONTROL,
            vk::LCONTROL,
            vk::MENU,
            vk::RMENU,
        ] {
            assert_eq!(all[usize::from(code)], 0x80, "vk {code:#x}");
        }
        assert_eq!(all[usize::from(vk::CAPITAL)], 0x01);
        assert_eq!(all[usize::from(vk::RSHIFT)], 0);
        assert_eq!(all.iter().filter(|&&b| b != 0).count(), 7);
    }

    #[test]
    fn decode_follows_the_return_value() {
        let buf = [0x61u16, 0x62, 0x63, 0];
        assert_eq!(Translation::decode(1, &buf), chars("a"));
        assert_eq!(Translation::decode(2, &buf), chars("ab"));
        assert_eq!(Translation::decode(0, &buf), Translation::None);
        // Dead key: the spacing acute accent U+00B4.
        assert_eq!(
            Translation::decode(-1, &[0x00B4, 0x0041]),
            Translation::Dead("\u{B4}".into())
        );
        // More units claimed than the buffer holds: clamped, no panic.
        assert_eq!(Translation::decode(9, &[0x61]), chars("a"));
        // A surrogate pair (U+1D400, two units).
        assert_eq!(
            Translation::decode(2, &[0xD835, 0xDC00]),
            chars("\u{1D400}")
        );
        // A lone surrogate becomes the replacement character, not a panic.
        assert_eq!(Translation::decode(1, &[0xD835]), chars("\u{FFFD}"));
        // Ctrl combinations give control characters: no text.
        assert_eq!(Translation::decode(1, &[0x0001]), Translation::None);
        assert!(Translation::Dead("^".into()).is_dead());
        assert_eq!(
            Translation::Dead("^".into()).into_text().as_deref(),
            Some("^")
        );
        assert_eq!(Translation::None.into_text(), None);
    }

    #[test]
    fn keycap_legend_rules() {
        assert_eq!(
            keycap_legend(&chars("a"), &chars("A")).as_deref(),
            Some("A")
        );
        assert_eq!(
            keycap_legend(&chars("ü"), &chars("Ü")).as_deref(),
            Some("Ü")
        );
        assert_eq!(
            keycap_legend(&chars("ß"), &chars("?")).as_deref(),
            Some("ß")
        );
        assert_eq!(
            keycap_legend(&chars(";"), &chars(":")).as_deref(),
            Some(";")
        );
        // AZERTY digit row: é unshifted, 2 shifted.
        assert_eq!(
            keycap_legend(&chars("é"), &chars("2")).as_deref(),
            Some("é")
        );
        assert_eq!(
            keycap_legend(&Translation::Dead("^".into()), &chars("¨")).as_deref(),
            Some("^")
        );
        assert_eq!(keycap_legend(&chars(" "), &chars(" ")), None);
        assert_eq!(keycap_legend(&Translation::None, &chars("A")), None);
    }

    #[test]
    fn klid_from_handles() {
        assert_eq!(klid_from_hkl(0x0409_0409), "00000409");
        assert_eq!(klid_from_hkl(0x0407_0409), "00000407");
        assert_eq!(klid_from_hkl(0x040C_040C), "0000040C");
        assert_eq!(klid_from_hkl(0x0409), "00000409");
        assert_eq!(klid_from_hkl(0xF002_0409), "F0020409");
        assert_eq!(klid_from_hkl(0xE001_0411), "E0010411");
        // Sign-extended 64-bit handles keep only the 32 meaningful bits.
        assert_eq!(
            klid_from_hkl(0xFFFF_FFFF_F002_0409_u64 as usize),
            "F0020409"
        );
    }

    #[test]
    fn us_table_matches_the_core_us_provider() {
        let t = LegendTable::us();
        assert_eq!(t.layout(), "00000409");
        let us = UsLegends;
        for &scan in keys::CHARACTER_SCANS {
            let code = us_scan_to_vk(scan).expect("US scan");
            assert_eq!(
                t.legend(code, p(scan)),
                us.legend(code, p(scan)),
                "scan {scan:#x}"
            );
            for i in 0..TEXT_STATES {
                let m = mods_for_index(i);
                assert_eq!(t.text(code, p(scan), m), us.text(code, p(scan), m));
            }
        }
        let wire = t.wire();
        let expected = build_legends(
            UsLegends::LAYOUT_ID,
            keys::CHARACTER_SCANS
                .iter()
                .filter_map(|s| us_scan_to_vk(*s).map(|v| (p(*s), v))),
            &UsLegends,
        );
        assert_eq!(wire, expected);
        assert_eq!(wire.legends.get("sc:1E").map(String::as_str), Some("A"));
        assert!(!wire.legends.contains_key("sc:39"), "Space has no legend");
    }

    #[test]
    fn numpad_keys_type_digits_but_have_no_keycap_legend() {
        let t = LegendTable::us();
        let plain = TextMods::default();
        assert_eq!(t.text(vk::NUMPAD7, p(0x47), plain).as_deref(), Some("7"));
        assert_eq!(t.text(vk::DECIMAL, p(0x53), plain).as_deref(), Some("."));
        assert_eq!(
            t.text(vk::DIVIDE, PhysKey::new(0x35, true), plain)
                .as_deref(),
            Some("/")
        );
        assert_eq!(t.legend(vk::NUMPAD7, p(0x47)), None);
        // Num Lock off: the same scan code sends VK_HOME, which types nothing.
        assert_eq!(t.text(vk::HOME, p(0x47), plain), None);
        assert!(!t.wire().legends.contains_key("sc:47"));
    }

    #[test]
    fn lookup_prefers_physical_key_then_virtual_key() {
        // A French AZERTY-like table: the key at 0x10 types A (VK_A).
        let entry = |scan: u16, code: u16, legend: &str| KeyEntry {
            phys: p(scan),
            vk: code,
            keycap: true,
            legend: Some(legend.to_string()),
            dead: false,
            text: std::array::from_fn(|i| {
                Some(if mods_for_index(i).shift {
                    legend.to_string()
                } else {
                    legend.to_lowercase()
                })
            }),
        };
        let t = LegendTable::new(
            "0000040C",
            vec![entry(0x10, 0x41, "A"), entry(0x1E, 0x51, "Q")],
        );
        assert_eq!(t.legend(0x41, p(0x10)).as_deref(), Some("A"));
        assert_eq!(t.legend(0x51, p(0x1E)).as_deref(), Some("Q"));
        // Injected VK_A with the US scan code of A: found by VK.
        assert_eq!(t.legend(0x41, p(0x1E)).as_deref(), Some("A"));
        assert_eq!(
            t.text(0x41, p(0x10), TextMods::default()).as_deref(),
            Some("a")
        );
        assert_eq!(t.legend(0x5A, p(0x2C)), None);
        assert_eq!(t.text(0x5A, p(0x2C), TextMods::default()), None);
        let w = t.wire();
        assert_eq!(w.layout, "0000040C");
        assert_eq!(w.legends.get("sc:10").map(String::as_str), Some("A"));
        assert_eq!(w.legends.get("sc:1E").map(String::as_str), Some("Q"));
    }

    #[test]
    fn dead_keys_are_flagged_and_type_their_accent() {
        let mut text: [Option<String>; TEXT_STATES] = Default::default();
        text[0] = Some("^".into());
        text[1] = Some("°".into());
        let t = LegendTable::new(
            "00000407",
            vec![KeyEntry {
                phys: p(0x29),
                vk: vk::OEM_5,
                keycap: true,
                legend: Some("^".into()),
                dead: true,
                text,
            }],
        );
        assert!(t.is_dead(vk::OEM_5, p(0x29)));
        assert!(!t.is_dead(vk::OEM_1, p(0x27)));
        assert_eq!(
            t.text(vk::OEM_5, p(0x29), TextMods::default()).as_deref(),
            Some("^")
        );
        assert_eq!(t.wire().legends.get("sc:29").map(String::as_str), Some("^"));
        assert_eq!(t.entries().len(), 1);
    }
}
