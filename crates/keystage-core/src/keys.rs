//! Canonical key names, the Windows virtual-key table, scan code helpers and
//! display labels (ARCHITECTURE.md section 3).
//!
//! The virtual-key values below are taken from Microsoft's documented list,
//! "Virtual-Key Codes", <https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
//! (page dated 2025-10-17, read 2026-10-01). Scan codes come from the "Scan
//! Codes" table of the Keyboard Input Overview,
//! <https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input>
//! (read 2026-10-01). Ranges the page marks reserved, unassigned or OEM
//! specific have no name and are reported as `VK_xx`.

use std::borrow::Cow;
use std::fmt;

use crate::event::{PhysKey, Side};
use crate::modifiers::{ModKey, ModSet};

/// Virtual-key codes used by the core, named as in winuser.h.
pub mod vk {
    pub const LBUTTON: u16 = 0x01;
    pub const RBUTTON: u16 = 0x02;
    pub const CANCEL: u16 = 0x03;
    pub const MBUTTON: u16 = 0x04;
    pub const XBUTTON1: u16 = 0x05;
    pub const XBUTTON2: u16 = 0x06;
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const CLEAR: u16 = 0x0C;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12;
    pub const PAUSE: u16 = 0x13;
    pub const CAPITAL: u16 = 0x14;
    pub const KANA: u16 = 0x15;
    pub const IME_ON: u16 = 0x16;
    pub const JUNJA: u16 = 0x17;
    pub const FINAL: u16 = 0x18;
    pub const HANJA: u16 = 0x19;
    pub const IME_OFF: u16 = 0x1A;
    pub const ESCAPE: u16 = 0x1B;
    pub const CONVERT: u16 = 0x1C;
    pub const NONCONVERT: u16 = 0x1D;
    pub const ACCEPT: u16 = 0x1E;
    pub const MODECHANGE: u16 = 0x1F;
    pub const SPACE: u16 = 0x20;
    pub const PRIOR: u16 = 0x21;
    pub const NEXT: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const SELECT: u16 = 0x29;
    pub const PRINT: u16 = 0x2A;
    pub const EXECUTE: u16 = 0x2B;
    pub const SNAPSHOT: u16 = 0x2C;
    pub const INSERT: u16 = 0x2D;
    pub const DELETE: u16 = 0x2E;
    pub const HELP: u16 = 0x2F;
    /// `0` to `9` are 0x30 to 0x39; `A` to `Z` are 0x41 to 0x5A.
    pub const KEY_0: u16 = 0x30;
    pub const KEY_9: u16 = 0x39;
    pub const KEY_A: u16 = 0x41;
    pub const KEY_Z: u16 = 0x5A;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const APPS: u16 = 0x5D;
    pub const SLEEP: u16 = 0x5F;
    pub const NUMPAD0: u16 = 0x60;
    pub const NUMPAD1: u16 = 0x61;
    pub const NUMPAD2: u16 = 0x62;
    pub const NUMPAD3: u16 = 0x63;
    pub const NUMPAD4: u16 = 0x64;
    pub const NUMPAD5: u16 = 0x65;
    pub const NUMPAD6: u16 = 0x66;
    pub const NUMPAD7: u16 = 0x67;
    pub const NUMPAD8: u16 = 0x68;
    pub const NUMPAD9: u16 = 0x69;
    pub const MULTIPLY: u16 = 0x6A;
    pub const ADD: u16 = 0x6B;
    pub const SEPARATOR: u16 = 0x6C;
    pub const SUBTRACT: u16 = 0x6D;
    pub const DECIMAL: u16 = 0x6E;
    pub const DIVIDE: u16 = 0x6F;
    pub const F1: u16 = 0x70;
    pub const F24: u16 = 0x87;
    pub const NUMLOCK: u16 = 0x90;
    pub const SCROLL: u16 = 0x91;
    pub const LSHIFT: u16 = 0xA0;
    pub const RSHIFT: u16 = 0xA1;
    pub const LCONTROL: u16 = 0xA2;
    pub const RCONTROL: u16 = 0xA3;
    pub const LMENU: u16 = 0xA4;
    pub const RMENU: u16 = 0xA5;
    pub const BROWSER_BACK: u16 = 0xA6;
    pub const BROWSER_FORWARD: u16 = 0xA7;
    pub const BROWSER_REFRESH: u16 = 0xA8;
    pub const BROWSER_STOP: u16 = 0xA9;
    pub const BROWSER_SEARCH: u16 = 0xAA;
    pub const BROWSER_FAVORITES: u16 = 0xAB;
    pub const BROWSER_HOME: u16 = 0xAC;
    pub const VOLUME_MUTE: u16 = 0xAD;
    pub const VOLUME_DOWN: u16 = 0xAE;
    pub const VOLUME_UP: u16 = 0xAF;
    pub const MEDIA_NEXT_TRACK: u16 = 0xB0;
    pub const MEDIA_PREV_TRACK: u16 = 0xB1;
    pub const MEDIA_STOP: u16 = 0xB2;
    pub const MEDIA_PLAY_PAUSE: u16 = 0xB3;
    pub const LAUNCH_MAIL: u16 = 0xB4;
    pub const LAUNCH_MEDIA_SELECT: u16 = 0xB5;
    pub const LAUNCH_APP1: u16 = 0xB6;
    pub const LAUNCH_APP2: u16 = 0xB7;
    pub const OEM_1: u16 = 0xBA;
    pub const OEM_PLUS: u16 = 0xBB;
    pub const OEM_COMMA: u16 = 0xBC;
    pub const OEM_MINUS: u16 = 0xBD;
    pub const OEM_PERIOD: u16 = 0xBE;
    pub const OEM_2: u16 = 0xBF;
    pub const OEM_3: u16 = 0xC0;
    pub const GAMEPAD_FIRST: u16 = 0xC3;
    pub const GAMEPAD_LAST: u16 = 0xDA;
    pub const OEM_4: u16 = 0xDB;
    pub const OEM_5: u16 = 0xDC;
    pub const OEM_6: u16 = 0xDD;
    pub const OEM_7: u16 = 0xDE;
    pub const OEM_8: u16 = 0xDF;
    pub const OEM_102: u16 = 0xE2;
    pub const PROCESSKEY: u16 = 0xE5;
    pub const PACKET: u16 = 0xE7;
    pub const ATTN: u16 = 0xF6;
    pub const CRSEL: u16 = 0xF7;
    pub const EXSEL: u16 = 0xF8;
    pub const EREOF: u16 = 0xF9;
    pub const PLAY: u16 = 0xFA;
    pub const ZOOM: u16 = 0xFB;
    pub const NONAME: u16 = 0xFC;
    pub const PA1: u16 = 0xFD;
    pub const OEM_CLEAR: u16 = 0xFE;
}

/// Set 1 scan codes the core relies on (Keyboard Input Overview table).
pub mod scan {
    pub const LCTRL: u16 = 0x1D;
    /// The fake left Ctrl Windows sends for AltGr. Not documented by
    /// Microsoft; reported by Keyman (<https://github.com/keymanapp/keyman/pull/14909>,
    /// "this is not a documented value"). The processor also keeps a
    /// same-timestamp fallback (see `Processor`).
    pub const ALTGR_FAKE_CTRL: u16 = 0x21D;
    pub const LSHIFT: u16 = 0x2A;
    pub const RSHIFT: u16 = 0x36;
    pub const ALT: u16 = 0x38;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const ENTER: u16 = 0x1C;
    /// Numpad scan codes shared with the navigation cluster (which sends
    /// them with the E0 prefix): 7 Home, 8 Up, 9 PgUp, 4 Left, 5 Clear,
    /// 6 Right, 1 End, 2 Down, 3 PgDn, 0 Ins, . Del.
    pub const NUMPAD_NAV: [u16; 11] = [
        0x47, 0x48, 0x49, 0x4B, 0x4C, 0x4D, 0x4F, 0x50, 0x51, 0x52, 0x53,
    ];
}

/// True for the fake left Ctrl that Windows sends ahead of AltGr, recognized
/// by its scan code 0x21D (undocumented; see [`scan::ALTGR_FAKE_CTRL`]). The
/// processor also recognizes it by timestamp when the scan code differs.
pub fn is_altgr_fake_ctrl(code: u16, phys: PhysKey) -> bool {
    matches!(code, vk::LCONTROL | vk::CONTROL)
        && !phys.extended
        && phys.scan == scan::ALTGR_FAKE_CTRL
}

/// How the processor treats a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyClass {
    /// Ctrl, Shift, Alt, Win (AltGr is decided by the processor).
    Modifier(ModKey, Side),
    /// A key that types a character: letters, digits, punctuation, Space,
    /// numpad digits and operators, and every unnamed or OEM specific code
    /// (unknown keys are treated as typing so masking fails closed).
    Character,
    /// Everything shown even when typing is masked: F-keys, Esc, Enter, Tab,
    /// Backspace, Delete, Insert, navigation, arrows, locks, PrintScreen,
    /// Pause, Menu, media, browser and launch keys, IME mode keys.
    Special,
}

/// A classified key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyInfo {
    /// Canonical name (section 3).
    pub name: Cow<'static, str>,
    pub class: KeyClass,
}

impl KeyInfo {
    pub fn is_modifier(&self) -> bool {
        matches!(self.class, KeyClass::Modifier(..))
    }
}

fn is_numpad_nav_scan(phys: PhysKey) -> bool {
    !phys.extended && scan::NUMPAD_NAV.contains(&phys.scan)
}

/// `VK_xx` name for a code with no canonical name.
pub fn vk_hex_name(code: u16) -> String {
    format!("VK_{:02X}", code)
}

const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
const LETTERS: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];
const NUMPAD_DIGITS: [&str; 10] = [
    "Num0", "Num1", "Num2", "Num3", "Num4", "Num5", "Num6", "Num7", "Num8", "Num9",
];
const FKEYS: [&str; 24] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15",
    "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
];

/// Classifies a key from its virtual-key code and physical key. The VK plus
/// the extended flag decides numpad versus navigation (section 3).
pub fn classify(code: u16, phys: PhysKey) -> KeyInfo {
    use KeyClass::{Character, Special};
    let st = |name: &'static str, class: KeyClass| KeyInfo {
        name: Cow::Borrowed(name),
        class,
    };
    let hex = |class: KeyClass| KeyInfo {
        name: Cow::Owned(vk_hex_name(code)),
        class,
    };
    // Numpad keys with Num Lock off send the navigation VKs without the
    // extended flag, from the numpad scan codes.
    let pad = is_numpad_nav_scan(phys);
    match code {
        vk::SHIFT => {
            let side = if phys.scan == scan::RSHIFT {
                Side::Right
            } else {
                Side::Left
            };
            st("Shift", KeyClass::Modifier(ModKey::Shift, side))
        }
        vk::LSHIFT => st("Shift", KeyClass::Modifier(ModKey::Shift, Side::Left)),
        vk::RSHIFT => st("Shift", KeyClass::Modifier(ModKey::Shift, Side::Right)),
        vk::CONTROL => {
            let side = if phys.extended {
                Side::Right
            } else {
                Side::Left
            };
            st("Ctrl", KeyClass::Modifier(ModKey::Ctrl, side))
        }
        vk::LCONTROL => st("Ctrl", KeyClass::Modifier(ModKey::Ctrl, Side::Left)),
        vk::RCONTROL => st("Ctrl", KeyClass::Modifier(ModKey::Ctrl, Side::Right)),
        vk::MENU => {
            let side = if phys.extended {
                Side::Right
            } else {
                Side::Left
            };
            st("Alt", KeyClass::Modifier(ModKey::Alt, side))
        }
        vk::LMENU => st("Alt", KeyClass::Modifier(ModKey::Alt, Side::Left)),
        vk::RMENU => st("Alt", KeyClass::Modifier(ModKey::Alt, Side::Right)),
        vk::LWIN => st("Win", KeyClass::Modifier(ModKey::Win, Side::Left)),
        vk::RWIN => st("Win", KeyClass::Modifier(ModKey::Win, Side::Right)),

        vk::BACK => st("Backspace", Special),
        vk::TAB => st("Tab", Special),
        vk::CLEAR if pad => st("NumClear", Special),
        vk::RETURN if phys.extended => st("NumEnter", Special),
        vk::RETURN => st("Enter", Special),
        vk::PAUSE => st("Pause", Special),
        // Ctrl+Pause sends VK_CANCEL ("Break key scan code is emitted on
        // Ctrl+Pause keystroke", Keyboard Input Overview); it is the Pause key.
        vk::CANCEL => st("Pause", Special),
        vk::CAPITAL => st("CapsLock", Special),
        vk::ESCAPE => st("Esc", Special),
        vk::SPACE => st("Space", Character),
        vk::PRIOR if pad => st("NumPageUp", Special),
        vk::PRIOR => st("PageUp", Special),
        vk::NEXT if pad => st("NumPageDown", Special),
        vk::NEXT => st("PageDown", Special),
        vk::END if pad => st("NumEnd", Special),
        vk::END => st("End", Special),
        vk::HOME if pad => st("NumHome", Special),
        vk::HOME => st("Home", Special),
        vk::LEFT if pad => st("NumLeft", Special),
        vk::LEFT => st("Left", Special),
        vk::UP if pad => st("NumUp", Special),
        vk::UP => st("Up", Special),
        vk::RIGHT if pad => st("NumRight", Special),
        vk::RIGHT => st("Right", Special),
        vk::DOWN if pad => st("NumDown", Special),
        vk::DOWN => st("Down", Special),
        vk::SNAPSHOT => st("PrintScreen", Special),
        vk::INSERT if pad => st("NumInsert", Special),
        vk::INSERT => st("Insert", Special),
        vk::DELETE if pad => st("NumDelete", Special),
        vk::DELETE => st("Delete", Special),
        vk::KEY_0..=vk::KEY_9 => st(DIGITS[(code - vk::KEY_0) as usize], Character),
        vk::KEY_A..=vk::KEY_Z => st(LETTERS[(code - vk::KEY_A) as usize], Character),
        vk::APPS => st("Menu", Special),
        vk::NUMPAD0..=vk::NUMPAD9 => st(NUMPAD_DIGITS[(code - vk::NUMPAD0) as usize], Character),
        vk::MULTIPLY => st("NumMultiply", Character),
        vk::ADD => st("NumAdd", Character),
        vk::SUBTRACT => st("NumSubtract", Character),
        vk::DECIMAL => st("NumDecimal", Character),
        vk::DIVIDE => st("NumDivide", Character),
        vk::SEPARATOR => hex(Character),
        vk::F1..=vk::F24 => st(FKEYS[(code - vk::F1) as usize], Special),
        vk::NUMLOCK => st("NumLock", Special),
        vk::SCROLL => st("ScrollLock", Special),
        vk::BROWSER_BACK => st("BrowserBack", Special),
        vk::BROWSER_FORWARD => st("BrowserForward", Special),
        vk::BROWSER_REFRESH => st("BrowserRefresh", Special),
        vk::BROWSER_SEARCH => st("BrowserSearch", Special),
        vk::BROWSER_HOME => st("BrowserHome", Special),
        vk::VOLUME_MUTE => st("VolumeMute", Special),
        vk::VOLUME_DOWN => st("VolumeDown", Special),
        vk::VOLUME_UP => st("VolumeUp", Special),
        vk::MEDIA_NEXT_TRACK => st("MediaNext", Special),
        vk::MEDIA_PREV_TRACK => st("MediaPrev", Special),
        vk::MEDIA_STOP => st("MediaStop", Special),
        vk::MEDIA_PLAY_PAUSE => st("MediaPlayPause", Special),
        vk::LAUNCH_MAIL => st("LaunchMail", Special),
        vk::LAUNCH_APP1 => st("LaunchApp1", Special),
        vk::LAUNCH_APP2 => st("LaunchApp2", Special),
        vk::OEM_1 => st(";", Character),
        vk::OEM_PLUS => st("=", Character),
        vk::OEM_COMMA => st(",", Character),
        vk::OEM_MINUS => st("-", Character),
        vk::OEM_PERIOD => st(".", Character),
        vk::OEM_2 => st("/", Character),
        vk::OEM_3 => st("`", Character),
        vk::OEM_4 => st("[", Character),
        vk::OEM_5 => st("\\", Character),
        vk::OEM_6 => st("]", Character),
        vk::OEM_7 => st("'", Character),
        vk::OEM_102 => st("IntlBackslash", Character),
        // Documented non-character keys without a canonical name.
        vk::LBUTTON | vk::RBUTTON | vk::MBUTTON | vk::XBUTTON1 | vk::XBUTTON2 => hex(Special),
        vk::KANA..=vk::IME_OFF | vk::CONVERT..=vk::MODECHANGE => hex(Special),
        vk::SELECT | vk::PRINT | vk::EXECUTE | vk::HELP | vk::SLEEP => hex(Special),
        vk::CLEAR | vk::BROWSER_STOP | vk::BROWSER_FAVORITES | vk::LAUNCH_MEDIA_SELECT => {
            hex(Special)
        }
        vk::GAMEPAD_FIRST..=vk::GAMEPAD_LAST => hex(Special),
        vk::ATTN..=vk::OEM_CLEAR => hex(Special),
        // VK_OEM_8, VK_PROCESSKEY (IME composition), VK_PACKET (Unicode
        // text from SendInput) and every reserved, unassigned or OEM
        // specific code: treated as typing, so masking fails closed.
        _ => hex(Character),
    }
}

/// True for the keys that make up typed values in SolidWorks (section 4):
/// digits, the numpad digits and operators, `.`, `,` and `-`.
pub fn is_value_key(name: &str) -> bool {
    matches!(
        name,
        "0" | "1"
            | "2"
            | "3"
            | "4"
            | "5"
            | "6"
            | "7"
            | "8"
            | "9"
            | "."
            | ","
            | "-"
            | "Num0"
            | "Num1"
            | "Num2"
            | "Num3"
            | "Num4"
            | "Num5"
            | "Num6"
            | "Num7"
            | "Num8"
            | "Num9"
            | "NumDecimal"
            | "NumSubtract"
            | "NumAdd"
            | "NumMultiply"
            | "NumDivide"
    )
}

/// True for Enter and numpad Enter.
pub fn is_enter(name: &str) -> bool {
    name == "Enter" || name == "NumEnter"
}

/// Every canonical name except `F1`..`F24`, letters, digits and `VK_xx`,
/// which [`is_canonical`] checks by pattern.
const NAMED: &[&str] = &[
    "Ctrl",
    "Shift",
    "Alt",
    "Win",
    "AltGr",
    "Space",
    "Enter",
    "Esc",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Up",
    "Down",
    "Left",
    "Right",
    "CapsLock",
    "PrintScreen",
    "ScrollLock",
    "Pause",
    "Menu",
    "NumLock",
    "Num0",
    "Num1",
    "Num2",
    "Num3",
    "Num4",
    "Num5",
    "Num6",
    "Num7",
    "Num8",
    "Num9",
    "NumAdd",
    "NumSubtract",
    "NumMultiply",
    "NumDivide",
    "NumDecimal",
    "NumEnter",
    "NumHome",
    "NumEnd",
    "NumPageUp",
    "NumPageDown",
    "NumUp",
    "NumDown",
    "NumLeft",
    "NumRight",
    "NumInsert",
    "NumDelete",
    "NumClear",
    "`",
    "-",
    "=",
    "[",
    "]",
    "\\",
    ";",
    "'",
    ",",
    ".",
    "/",
    "VolumeUp",
    "VolumeDown",
    "VolumeMute",
    "MediaPlayPause",
    "MediaNext",
    "MediaPrev",
    "MediaStop",
    "BrowserBack",
    "BrowserForward",
    "BrowserRefresh",
    "BrowserHome",
    "BrowserSearch",
    "LaunchMail",
    "LaunchApp1",
    "LaunchApp2",
    "IntlBackslash",
];

/// True if `name` is a canonical key name from section 3.
pub fn is_canonical(name: &str) -> bool {
    if NAMED.contains(&name) {
        return true;
    }
    if name.len() == 1 {
        let c = name.as_bytes()[0];
        return c.is_ascii_uppercase() || c.is_ascii_digit();
    }
    if let Some(n) = name.strip_prefix('F') {
        if let Ok(v) = n.parse::<u8>() {
            return (1..=24).contains(&v) && !n.starts_with('0');
        }
    }
    if let Some(h) = name.strip_prefix("VK_") {
        return h.len() == 2
            && h.chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
            && u8::from_str_radix(h, 16).is_ok();
    }
    false
}

/// True for the canonical names of modifier keys.
pub fn is_modifier_name(name: &str) -> bool {
    ModKey::from_name(name).is_some()
}

/// Display label for a canonical key name, without a layout. Character keys
/// should be labeled through a [`crate::LegendProvider`] first; this is the
/// fallback and the label for every named key. With `symbols`, arrows,
/// Enter, Tab and Backspace become glyphs (the overlay's "symbols" option).
pub fn display_label(name: &str, symbols: bool) -> Cow<'_, str> {
    if symbols {
        if let Some(g) = symbol_label(name) {
            return Cow::Borrowed(g);
        }
    }
    Cow::Borrowed(word_label(name))
}

/// The glyph form of arrows, Enter, Tab and Backspace, if `name` has one.
pub fn symbol_label(name: &str) -> Option<&'static str> {
    Some(match name {
        "Up" => "\u{2191}",
        "Down" => "\u{2193}",
        "Left" => "\u{2190}",
        "Right" => "\u{2192}",
        "Enter" => "\u{21B5}",
        "Tab" => "\u{21E5}",
        "Backspace" => "\u{232B}",
        "NumUp" => "Num \u{2191}",
        "NumDown" => "Num \u{2193}",
        "NumLeft" => "Num \u{2190}",
        "NumRight" => "Num \u{2192}",
        "NumEnter" => "Num \u{21B5}",
        _ => return None,
    })
}

/// The word label of a canonical name. Named keys use the words printed on
/// keycaps (`Page Up`); everything else is returned as is.
pub fn word_label(name: &str) -> &str {
    match name {
        "PageUp" => "Page Up",
        "PageDown" => "Page Down",
        "CapsLock" => "Caps Lock",
        "PrintScreen" => "Print Screen",
        "ScrollLock" => "Scroll Lock",
        "NumLock" => "Num Lock",
        "Num0" => "Num 0",
        "Num1" => "Num 1",
        "Num2" => "Num 2",
        "Num3" => "Num 3",
        "Num4" => "Num 4",
        "Num5" => "Num 5",
        "Num6" => "Num 6",
        "Num7" => "Num 7",
        "Num8" => "Num 8",
        "Num9" => "Num 9",
        "NumAdd" => "Num +",
        "NumSubtract" => "Num -",
        "NumMultiply" => "Num *",
        "NumDivide" => "Num /",
        "NumDecimal" => "Num .",
        "NumEnter" => "Num Enter",
        "NumHome" => "Num Home",
        "NumEnd" => "Num End",
        "NumPageUp" => "Num Page Up",
        "NumPageDown" => "Num Page Down",
        "NumUp" => "Num Up",
        "NumDown" => "Num Down",
        "NumLeft" => "Num Left",
        "NumRight" => "Num Right",
        "NumInsert" => "Num Insert",
        "NumDelete" => "Num Delete",
        "NumClear" => "Num Clear",
        "VolumeUp" => "Volume Up",
        "VolumeDown" => "Volume Down",
        "VolumeMute" => "Mute",
        "MediaPlayPause" => "Play/Pause",
        "MediaNext" => "Next Track",
        "MediaPrev" => "Previous Track",
        "MediaStop" => "Stop",
        "BrowserBack" => "Browser Back",
        "BrowserForward" => "Browser Forward",
        "BrowserRefresh" => "Browser Refresh",
        "BrowserHome" => "Browser Home",
        "BrowserSearch" => "Browser Search",
        "LaunchMail" => "Mail",
        "LaunchApp1" => "App 1",
        "LaunchApp2" => "App 2",
        "IntlBackslash" => "\\",
        other => other,
    }
}

/// The physical key a modifier normally sits on, for keys the core never saw
/// go down (a [`crate::InputEvent::ModifierResync`] that reports a held key).
pub fn modifier_phys(m: ModKey, side: Side) -> PhysKey {
    match (m, side) {
        (ModKey::Ctrl, Side::Left) => PhysKey::new(scan::LCTRL, false),
        (ModKey::Ctrl, Side::Right) => PhysKey::new(scan::LCTRL, true),
        (ModKey::Shift, Side::Left) => PhysKey::new(scan::LSHIFT, false),
        (ModKey::Shift, Side::Right) => PhysKey::new(scan::RSHIFT, false),
        (ModKey::Alt, Side::Left) => PhysKey::new(scan::ALT, false),
        (ModKey::Alt, Side::Right) | (ModKey::AltGr, _) => PhysKey::new(scan::ALT, true),
        (ModKey::Win, Side::Left) => PhysKey::new(scan::LWIN, true),
        (ModKey::Win, Side::Right) => PhysKey::new(scan::RWIN, true),
    }
}

/// Scan codes (set 1, not extended) of the character block of a 104 or 105
/// key keyboard: the digit row, the three letter rows, the 102nd key and
/// Space. The platform layer maps each through the active layout
/// (`MapVirtualKeyExW(..., MAPVK_VSC_TO_VK_EX, hkl)`) and asks its
/// [`crate::LegendProvider`] for the legend to build a `legends` message.
pub const CHARACTER_SCANS: &[u16] = &[
    0x29, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
    0x0D, // ` 1..0 - =
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
    0x2B, // Q..P [ ] \
    0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, // A..L ; '
    0x56, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, // 102nd Z..M , . /
    0x39, // Space
];

/// A parsed accelerator such as `Ctrl+Alt+Shift+P`: a modifier set plus one
/// canonical key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Accelerator {
    pub mods: ModSet,
    pub key: String,
}

/// Modifier order of accelerator strings, as the contract writes the
/// default hotkeys (`Ctrl+Alt+Shift+P`).
const ACCEL_ORDER: [ModKey; 5] = [
    ModKey::Ctrl,
    ModKey::Alt,
    ModKey::AltGr,
    ModKey::Shift,
    ModKey::Win,
];

impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in ACCEL_ORDER.iter().filter(|m| self.mods.contains(**m)) {
            write!(f, "{}+", m.name())?;
        }
        f.write_str(&self.key)
    }
}

impl Accelerator {
    /// The string Tauri's global-shortcut plugin parses (global-hotkey
    /// 0.8.0 `parse_hotkey`: modifiers `Ctrl`, `Alt`, `Shift`, `Super`, then
    /// one key). `None` if the plugin cannot register this combination
    /// (AltGr, or a key it has no name for, such as `Menu`).
    pub fn to_global_shortcut(&self) -> Option<String> {
        if self.mods.contains(ModKey::AltGr) {
            return None;
        }
        let key = match self.key.as_str() {
            "MediaNext" => "MediaTrackNext",
            "MediaPrev" => "MediaTrackPrevious",
            k if k.len() == 1 && "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789`\\[],-.=';/".contains(k) => {
                k
            }
            k if k.starts_with('F')
                && k.len() > 1
                && k[1..].chars().all(|c| c.is_ascii_digit()) =>
            {
                k
            }
            k if k.starts_with("Num") && k.len() == 4 && k.as_bytes()[3].is_ascii_digit() => k,
            k @ ("Space" | "Enter" | "Tab" | "Esc" | "Backspace" | "Delete" | "Insert" | "Home"
            | "End" | "PageUp" | "PageDown" | "Up" | "Down" | "Left" | "Right"
            | "CapsLock" | "PrintScreen" | "ScrollLock" | "Pause" | "NumLock" | "NumAdd"
            | "NumSubtract" | "NumMultiply" | "NumDivide" | "NumDecimal" | "NumEnter"
            | "VolumeUp" | "VolumeDown" | "VolumeMute" | "MediaPlayPause" | "MediaStop") => k,
            _ => return None,
        };
        let mut s = String::new();
        for (m, word) in [
            (ModKey::Ctrl, "Ctrl"),
            (ModKey::Alt, "Alt"),
            (ModKey::Shift, "Shift"),
            (ModKey::Win, "Super"),
        ] {
            if self.mods.contains(m) {
                s.push_str(word);
                s.push('+');
            }
        }
        s.push_str(key);
        Some(s)
    }
}

/// Why an accelerator string could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AcceleratorError {
    #[error("the shortcut is empty")]
    Empty,
    #[error("the shortcut has no key besides modifiers")]
    NoKey,
    #[error("the shortcut has more than one key")]
    SeveralKeys,
    #[error("unknown key name \"{0}\"")]
    UnknownKey(String),
}

/// Reads one key name the way people and Tauri accelerators write it
/// (`Escape`, `PgUp`, `ArrowUp`, `KeyP`, `Digit1`, `Spacebar`, lowercase
/// letters) and returns the canonical name.
pub fn parse_key_name(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if is_canonical(t) {
        return Some(t.to_string());
    }
    if t.chars().count() == 1 {
        let c = t.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(c.to_ascii_uppercase().to_string());
        }
        return None;
    }
    let compact: String = t
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .collect();
    let lower = compact.to_ascii_lowercase();
    if let Some(m) = ModKey::from_alias(&lower) {
        return Some(m.name().to_string());
    }
    let named = match lower.as_str() {
        "space" | "spacebar" => "Space",
        "enter" | "return" => "Enter",
        "esc" | "escape" => "Esc",
        "tab" => "Tab",
        "backspace" | "back" | "bksp" => "Backspace",
        "delete" | "del" => "Delete",
        "insert" | "ins" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" | "pgup" | "prior" => "PageUp",
        "pagedown" | "pgdn" | "pgdown" | "next" => "PageDown",
        "up" | "arrowup" | "uparrow" => "Up",
        "down" | "arrowdown" | "downarrow" => "Down",
        "left" | "arrowleft" | "leftarrow" => "Left",
        "right" | "arrowright" | "rightarrow" => "Right",
        "capslock" | "caps" => "CapsLock",
        "printscreen" | "prtsc" | "prtscn" | "print" | "snapshot" => "PrintScreen",
        "scrolllock" | "scroll" => "ScrollLock",
        "pause" | "break" => "Pause",
        "menu" | "apps" | "application" | "contextmenu" | "menukey" => "Menu",
        "numlock" => "NumLock",
        "backquote" | "grave" | "backtick" => "`",
        "minus" | "dash" | "hyphen" => "-",
        "equal" | "equals" => "=",
        "bracketleft" | "leftbracket" => "[",
        "bracketright" | "rightbracket" => "]",
        "backslash" => "\\",
        "semicolon" => ";",
        "quote" | "apostrophe" => "'",
        "comma" => ",",
        "period" | "dot" => ".",
        "slash" => "/",
        "intlbackslash" => "IntlBackslash",
        "numpadenter" | "numenter" => "NumEnter",
        "numpadadd" | "numadd" | "num+" => "NumAdd",
        "numpadsubtract" | "numsubtract" | "num-" => "NumSubtract",
        "numpadmultiply" | "nummultiply" | "num*" => "NumMultiply",
        "numpaddivide" | "numdivide" | "num/" => "NumDivide",
        "numpaddecimal" | "numdecimal" | "num." => "NumDecimal",
        "audiovolumeup" | "volumeup" => "VolumeUp",
        "audiovolumedown" | "volumedown" => "VolumeDown",
        "audiovolumemute" | "volumemute" | "mute" => "VolumeMute",
        "mediaplaypause" | "playpause" => "MediaPlayPause",
        "mediatracknext" | "medianext" => "MediaNext",
        "mediatrackprevious" | "mediaprev" => "MediaPrev",
        "mediastop" => "MediaStop",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.to_string());
    }
    if let Some(rest) = lower.strip_prefix("key") {
        if rest.len() == 1 && rest.as_bytes()[0].is_ascii_lowercase() {
            return Some(rest.to_ascii_uppercase());
        }
    }
    if let Some(rest) = lower.strip_prefix("digit") {
        if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
            return Some(rest.to_string());
        }
    }
    for prefix in ["numpad", "num"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
                return Some(format!("Num{rest}"));
            }
        }
    }
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(v) = rest.parse::<u8>() {
            if (1..=24).contains(&v) && !rest.starts_with('0') {
                return Some(format!("F{v}"));
            }
        }
    }
    None
}

/// Parses an accelerator string such as `Ctrl+Alt+Shift+P`,
/// `CommandOrControl+Shift+K` or `ctrl + alt + shift + p`.
pub fn parse_accelerator(s: &str) -> Result<Accelerator, AcceleratorError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(AcceleratorError::Empty);
    }
    let mut mods = ModSet::EMPTY;
    let mut key: Option<String> = None;
    for part in split_plus(trimmed) {
        let part = part.trim();
        if part.is_empty() {
            return Err(AcceleratorError::UnknownKey(String::new()));
        }
        let compact: String = part.chars().filter(|c| !c.is_whitespace()).collect();
        if let Some(m) = ModKey::from_alias(&compact.to_ascii_lowercase()) {
            mods.insert(m);
            continue;
        }
        let name = parse_key_name(part).ok_or_else(|| AcceleratorError::UnknownKey(part.into()))?;
        if key.is_some() {
            return Err(AcceleratorError::SeveralKeys);
        }
        key = Some(name);
    }
    match key {
        Some(key) => Ok(Accelerator { mods, key }),
        None => Err(AcceleratorError::NoKey),
    }
}

/// Splits on `+`, keeping a trailing `+` as the key itself is not supported
/// (no canonical key is named `+`; the US `+` key is `=`).
fn split_plus(s: &str) -> Vec<&str> {
    s.split('+').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(scan: u16, ext: bool) -> PhysKey {
        PhysKey::new(scan, ext)
    }

    fn name(code: u16, phys: PhysKey) -> String {
        classify(code, phys).name.into_owned()
    }

    #[test]
    fn letters_digits_and_function_keys() {
        assert_eq!(name(0x41, p(0x1E, false)), "A");
        assert_eq!(name(0x5A, p(0x2C, false)), "Z");
        assert_eq!(name(0x30, p(0x0B, false)), "0");
        assert_eq!(name(0x39, p(0x0A, false)), "9");
        assert_eq!(name(0x70, p(0x3B, false)), "F1");
        assert_eq!(name(0x7B, p(0x58, false)), "F12");
        assert_eq!(name(0x87, p(0x76, false)), "F24");
        assert_eq!(classify(0x41, p(0x1E, false)).class, KeyClass::Character);
        assert_eq!(classify(0x70, p(0x3B, false)).class, KeyClass::Special);
    }

    #[test]
    fn numpad_versus_navigation() {
        // Navigation cluster: extended.
        assert_eq!(name(vk::HOME, p(0x47, true)), "Home");
        assert_eq!(name(vk::END, p(0x4F, true)), "End");
        assert_eq!(name(vk::PRIOR, p(0x49, true)), "PageUp");
        assert_eq!(name(vk::NEXT, p(0x51, true)), "PageDown");
        assert_eq!(name(vk::UP, p(0x48, true)), "Up");
        assert_eq!(name(vk::DOWN, p(0x50, true)), "Down");
        assert_eq!(name(vk::LEFT, p(0x4B, true)), "Left");
        assert_eq!(name(vk::RIGHT, p(0x4D, true)), "Right");
        assert_eq!(name(vk::INSERT, p(0x52, true)), "Insert");
        assert_eq!(name(vk::DELETE, p(0x53, true)), "Delete");
        // Numpad with Num Lock off: same VKs, not extended, numpad scans.
        assert_eq!(name(vk::HOME, p(0x47, false)), "NumHome");
        assert_eq!(name(vk::END, p(0x4F, false)), "NumEnd");
        assert_eq!(name(vk::PRIOR, p(0x49, false)), "NumPageUp");
        assert_eq!(name(vk::NEXT, p(0x51, false)), "NumPageDown");
        assert_eq!(name(vk::UP, p(0x48, false)), "NumUp");
        assert_eq!(name(vk::DOWN, p(0x50, false)), "NumDown");
        assert_eq!(name(vk::LEFT, p(0x4B, false)), "NumLeft");
        assert_eq!(name(vk::RIGHT, p(0x4D, false)), "NumRight");
        assert_eq!(name(vk::INSERT, p(0x52, false)), "NumInsert");
        assert_eq!(name(vk::DELETE, p(0x53, false)), "NumDelete");
        assert_eq!(name(vk::CLEAR, p(0x4C, false)), "NumClear");
        // Injected with no scan code: the VK decides.
        assert_eq!(name(vk::HOME, p(0, false)), "Home");
        assert_eq!(name(vk::CLEAR, p(0, false)), "VK_0C");
        // Enter versus numpad Enter.
        assert_eq!(name(vk::RETURN, p(0x1C, false)), "Enter");
        assert_eq!(name(vk::RETURN, p(0x1C, true)), "NumEnter");
        // Numpad digits and operators.
        assert_eq!(name(vk::NUMPAD0, p(0x52, false)), "Num0");
        assert_eq!(name(vk::NUMPAD9, p(0x49, false)), "Num9");
        assert_eq!(name(vk::ADD, p(0x4E, false)), "NumAdd");
        assert_eq!(name(vk::SUBTRACT, p(0x4A, false)), "NumSubtract");
        assert_eq!(name(vk::MULTIPLY, p(0x37, false)), "NumMultiply");
        assert_eq!(name(vk::DIVIDE, p(0x35, true)), "NumDivide");
        assert_eq!(name(vk::DECIMAL, p(0x53, false)), "NumDecimal");
        assert_eq!(
            classify(vk::NUMPAD5, p(0x4C, false)).class,
            KeyClass::Character
        );
        assert_eq!(classify(vk::HOME, p(0x47, false)).class, KeyClass::Special);
    }

    #[test]
    fn modifiers_and_sides() {
        let m = |code, phys| classify(code, phys).class;
        assert_eq!(
            m(vk::LCONTROL, p(0x1D, false)),
            KeyClass::Modifier(ModKey::Ctrl, Side::Left)
        );
        assert_eq!(
            m(vk::RCONTROL, p(0x1D, true)),
            KeyClass::Modifier(ModKey::Ctrl, Side::Right)
        );
        assert_eq!(
            m(vk::CONTROL, p(0x1D, true)),
            KeyClass::Modifier(ModKey::Ctrl, Side::Right)
        );
        assert_eq!(
            m(vk::CONTROL, p(0x1D, false)),
            KeyClass::Modifier(ModKey::Ctrl, Side::Left)
        );
        assert_eq!(
            m(vk::SHIFT, p(0x36, false)),
            KeyClass::Modifier(ModKey::Shift, Side::Right)
        );
        assert_eq!(
            m(vk::SHIFT, p(0x2A, false)),
            KeyClass::Modifier(ModKey::Shift, Side::Left)
        );
        assert_eq!(
            m(vk::RSHIFT, p(0x36, false)),
            KeyClass::Modifier(ModKey::Shift, Side::Right)
        );
        assert_eq!(
            m(vk::MENU, p(0x38, true)),
            KeyClass::Modifier(ModKey::Alt, Side::Right)
        );
        assert_eq!(
            m(vk::LMENU, p(0x38, false)),
            KeyClass::Modifier(ModKey::Alt, Side::Left)
        );
        assert_eq!(
            m(vk::LWIN, p(0x5B, true)),
            KeyClass::Modifier(ModKey::Win, Side::Left)
        );
        assert_eq!(
            m(vk::RWIN, p(0x5C, true)),
            KeyClass::Modifier(ModKey::Win, Side::Right)
        );
        assert_eq!(name(vk::LWIN, p(0x5B, true)), "Win");
    }

    #[test]
    fn punctuation_media_and_unknown() {
        let cases = [
            (vk::OEM_1, ";"),
            (vk::OEM_PLUS, "="),
            (vk::OEM_COMMA, ","),
            (vk::OEM_MINUS, "-"),
            (vk::OEM_PERIOD, "."),
            (vk::OEM_2, "/"),
            (vk::OEM_3, "`"),
            (vk::OEM_4, "["),
            (vk::OEM_5, "\\"),
            (vk::OEM_6, "]"),
            (vk::OEM_7, "'"),
            (vk::OEM_102, "IntlBackslash"),
            (vk::VOLUME_UP, "VolumeUp"),
            (vk::VOLUME_DOWN, "VolumeDown"),
            (vk::VOLUME_MUTE, "VolumeMute"),
            (vk::MEDIA_PLAY_PAUSE, "MediaPlayPause"),
            (vk::MEDIA_NEXT_TRACK, "MediaNext"),
            (vk::MEDIA_PREV_TRACK, "MediaPrev"),
            (vk::MEDIA_STOP, "MediaStop"),
            (vk::BROWSER_BACK, "BrowserBack"),
            (vk::BROWSER_FORWARD, "BrowserForward"),
            (vk::BROWSER_REFRESH, "BrowserRefresh"),
            (vk::BROWSER_HOME, "BrowserHome"),
            (vk::BROWSER_SEARCH, "BrowserSearch"),
            (vk::LAUNCH_MAIL, "LaunchMail"),
            (vk::LAUNCH_APP1, "LaunchApp1"),
            (vk::LAUNCH_APP2, "LaunchApp2"),
            (vk::SNAPSHOT, "PrintScreen"),
            (vk::SCROLL, "ScrollLock"),
            (vk::PAUSE, "Pause"),
            (vk::CANCEL, "Pause"),
            (vk::APPS, "Menu"),
            (vk::NUMLOCK, "NumLock"),
            (vk::CAPITAL, "CapsLock"),
            (vk::ESCAPE, "Esc"),
            (vk::SPACE, "Space"),
            (vk::TAB, "Tab"),
            (vk::BACK, "Backspace"),
        ];
        for (code, expected) in cases {
            let n = name(code, p(0, false));
            assert_eq!(n, expected, "vk {code:#04x}");
            assert!(is_canonical(&n), "{n} must be canonical");
        }
        assert_eq!(
            classify(vk::VOLUME_UP, p(0, false)).class,
            KeyClass::Special
        );
        assert_eq!(name(vk::BROWSER_STOP, p(0, false)), "VK_A9");
        assert_eq!(name(vk::SEPARATOR, p(0, false)), "VK_6C");
        // Typing-like unknowns fail closed as characters.
        assert_eq!(classify(vk::PACKET, p(0, false)).class, KeyClass::Character);
        assert_eq!(
            classify(vk::PROCESSKEY, p(0, false)).class,
            KeyClass::Character
        );
        assert_eq!(classify(vk::OEM_8, p(0, false)).class, KeyClass::Character);
        assert_eq!(classify(0x92, p(0, false)).class, KeyClass::Character);
        assert_eq!(classify(0x07, p(0, false)).class, KeyClass::Character);
        assert_eq!(name(0xE7, p(0, false)), "VK_E7");
        // Documented non-character keys without a name are special.
        assert_eq!(classify(vk::HANJA, p(0, false)).class, KeyClass::Special);
        assert_eq!(classify(vk::SLEEP, p(0, false)).class, KeyClass::Special);
        assert_eq!(classify(vk::ZOOM, p(0, false)).class, KeyClass::Special);
        assert_eq!(classify(0xC5, p(0, false)).class, KeyClass::Special);
    }

    #[test]
    fn every_code_has_a_canonical_name() {
        for code in 0u16..=0xFF {
            for ext in [false, true] {
                for sc in [0u16, 0x47, 0x4C, 0x1C, 0x36] {
                    let n = name(code, p(sc, ext));
                    assert!(is_canonical(&n), "{code:#04x} gave {n}");
                }
            }
        }
    }

    #[test]
    fn canonical_names() {
        for n in [
            "Ctrl",
            "A",
            "0",
            "F1",
            "F24",
            "NumEnter",
            "VK_A9",
            "`",
            "IntlBackslash",
        ] {
            assert!(is_canonical(n), "{n}");
        }
        for n in [
            "ctrl", "a", "F0", "F25", "F01", "Return", "VK_ZZ", "VK_1", "", "Plus",
        ] {
            assert!(!is_canonical(n), "{n}");
        }
        assert!(is_modifier_name("AltGr"));
        assert!(!is_modifier_name("A"));
    }

    #[test]
    fn labels_and_symbols() {
        assert_eq!(display_label("PageUp", false), "Page Up");
        assert_eq!(display_label("Num7", false), "Num 7");
        assert_eq!(display_label("Up", false), "Up");
        assert_eq!(display_label("Up", true), "\u{2191}");
        assert_eq!(display_label("Enter", true), "\u{21B5}");
        assert_eq!(display_label("Tab", true), "\u{21E5}");
        assert_eq!(display_label("Backspace", true), "\u{232B}");
        assert_eq!(display_label("NumEnter", true), "Num \u{21B5}");
        assert_eq!(display_label("S", true), "S");
        assert_eq!(display_label("VolumeMute", false), "Mute");
        assert_eq!(display_label("IntlBackslash", false), "\\");
        for n in NAMED {
            assert!(!display_label(n, false).is_empty());
        }
    }

    #[test]
    fn value_keys() {
        for n in ["0", "9", ".", "-", ",", "Num5", "NumDecimal", "NumSubtract"] {
            assert!(is_value_key(n), "{n}");
        }
        for n in ["A", "=", "Space", "Enter", "/"] {
            assert!(!is_value_key(n), "{n}");
        }
    }

    #[test]
    fn altgr_fake_ctrl() {
        assert!(is_altgr_fake_ctrl(vk::LCONTROL, p(0x21D, false)));
        assert!(is_altgr_fake_ctrl(vk::CONTROL, p(0x21D, false)));
        assert!(!is_altgr_fake_ctrl(vk::LCONTROL, p(0x1D, false)));
        assert!(!is_altgr_fake_ctrl(vk::RCONTROL, p(0x21D, false)));
    }

    #[test]
    fn modifier_physical_keys() {
        assert_eq!(modifier_phys(ModKey::Ctrl, Side::Left).wire_id(), "sc:1D");
        assert_eq!(
            modifier_phys(ModKey::Ctrl, Side::Right).wire_id(),
            "sc:E01D"
        );
        assert_eq!(modifier_phys(ModKey::Shift, Side::Right).wire_id(), "sc:36");
        assert_eq!(
            modifier_phys(ModKey::AltGr, Side::Right).wire_id(),
            "sc:E038"
        );
        assert_eq!(modifier_phys(ModKey::Win, Side::Left).wire_id(), "sc:E05B");
    }

    #[test]
    fn accelerators() {
        let a = parse_accelerator("Ctrl+Alt+Shift+P").unwrap();
        assert_eq!(a.key, "P");
        assert!(a.mods.contains(ModKey::Ctrl));
        assert!(a.mods.contains(ModKey::Alt));
        assert!(a.mods.contains(ModKey::Shift));
        assert_eq!(a.to_string(), "Ctrl+Alt+Shift+P");
        assert_eq!(a.to_global_shortcut().as_deref(), Some("Ctrl+Alt+Shift+P"));
        let b = parse_accelerator(" control + alt + shift + p ").unwrap();
        assert_eq!(a, b);
        let c = parse_accelerator("CommandOrControl+Shift+KeyK").unwrap();
        assert_eq!(c.key, "K");
        assert!(c.mods.contains(ModKey::Ctrl) && c.mods.contains(ModKey::Shift));
        let d = parse_accelerator("Super+Digit1").unwrap();
        assert_eq!(d.key, "1");
        assert!(d.mods.contains(ModKey::Win));
        assert_eq!(d.to_string(), "Win+1");
        assert_eq!(d.to_global_shortcut().as_deref(), Some("Super+1"));
        assert_eq!(
            parse_accelerator("Ctrl+Menu").unwrap().to_global_shortcut(),
            None
        );
        assert_eq!(
            parse_accelerator("AltGr+Q").unwrap().to_global_shortcut(),
            None
        );
        assert_eq!(
            parse_accelerator("Ctrl+MediaNext")
                .unwrap()
                .to_global_shortcut()
                .as_deref(),
            Some("Ctrl+MediaTrackNext")
        );
        assert_eq!(
            parse_accelerator("Ctrl+Num5")
                .unwrap()
                .to_global_shortcut()
                .as_deref(),
            Some("Ctrl+Num5")
        );
        assert_eq!(
            parse_accelerator("Ctrl+F12")
                .unwrap()
                .to_global_shortcut()
                .as_deref(),
            Some("Ctrl+F12")
        );
        assert_eq!(
            parse_accelerator("Ctrl+NumHome")
                .unwrap()
                .to_global_shortcut(),
            None
        );
        assert_eq!(parse_accelerator("Alt+F4").unwrap().key, "F4");
        assert_eq!(parse_accelerator("Escape").unwrap().key, "Esc");
        assert_eq!(parse_accelerator("Ctrl+PgUp").unwrap().key, "PageUp");
        assert_eq!(parse_accelerator("Ctrl+ArrowUp").unwrap().key, "Up");
        assert_eq!(parse_accelerator(""), Err(AcceleratorError::Empty));
        assert_eq!(parse_accelerator("Ctrl+Alt"), Err(AcceleratorError::NoKey));
        assert_eq!(
            parse_accelerator("Ctrl+A+B"),
            Err(AcceleratorError::SeveralKeys)
        );
        assert!(matches!(
            parse_accelerator("Ctrl+Banana"),
            Err(AcceleratorError::UnknownKey(_))
        ));
        assert!(matches!(
            parse_accelerator("Ctrl+"),
            Err(AcceleratorError::UnknownKey(_))
        ));
    }

    #[test]
    fn key_name_aliases() {
        let cases = [
            ("Spacebar", "Space"),
            ("Space Bar", "Space"),
            ("Return", "Enter"),
            ("Escape", "Esc"),
            ("Page Down", "PageDown"),
            ("Up Arrow", "Up"),
            ("del", "Delete"),
            ("b", "B"),
            ("Num 5", "Num5"),
            ("Numpad5", "Num5"),
            ("f5", "F5"),
            ("Print Screen", "PrintScreen"),
            ("Scroll Lock", "ScrollLock"),
            ("Apps", "Menu"),
            ("Ctrl", "Ctrl"),
            ("Control", "Ctrl"),
        ];
        for (raw, expected) in cases {
            assert_eq!(parse_key_name(raw).as_deref(), Some(expected), "{raw}");
        }
        for raw in ["", "???", "F0", "F25", "Banana", "é"] {
            assert_eq!(parse_key_name(raw), None, "{raw}");
        }
    }

    #[test]
    fn character_scans_are_unique() {
        let mut v = CHARACTER_SCANS.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), CHARACTER_SCANS.len());
    }
}
