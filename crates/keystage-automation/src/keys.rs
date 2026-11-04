//! Keyboard plans: a chord such as `ctrl+shift+s` and typed text, as timed
//! key steps for `SendInput`. Characters go as `KEYEVENTF_UNICODE` (any
//! layout, any character); Enter, Tab, Escape and Backspace go as their
//! virtual keys, so applications see the real keys.

use serde::{Deserialize, Serialize};

use crate::motion::Rng;

/// One keyboard step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    /// A virtual key; `extended` sets `KEYEVENTF_EXTENDEDKEY`.
    Vk { vk: u16, extended: bool },
    /// One UTF-16 code unit sent with `KEYEVENTF_UNICODE`.
    Unicode(u16),
}

/// A key going down or up, `delay_ms` after the previous step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStep {
    pub delay_ms: u64,
    pub key: Key,
    pub down: bool,
}

pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_MENU: u16 = 0x12;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_TAB: u16 = 0x09;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_BACK: u16 = 0x08;

/// Longest text one `type_text` sends.
pub const MAX_TEXT_CHARS: usize = 10_000;

fn named(name: &str) -> Option<(u16, bool)> {
    let n = name.to_ascii_lowercase();
    let vk = |v: u16| Some((v, false));
    let ext = |v: u16| Some((v, true));
    if n.len() == 1 {
        let c = n.as_bytes()[0];
        return match c {
            b'a'..=b'z' => vk(u16::from(c - b'a' + b'A')),
            b'0'..=b'9' => vk(u16::from(c)),
            b'-' => vk(0xBD),
            b'=' => vk(0xBB),
            b',' => vk(0xBC),
            b'.' => vk(0xBE),
            b'/' => vk(0xBF),
            b';' => vk(0xBA),
            b'\'' => vk(0xDE),
            b'[' => vk(0xDB),
            b']' => vk(0xDD),
            b'\\' => vk(0xDC),
            b'`' => vk(0xC0),
            _ => None,
        };
    }
    if let Some(num) = n.strip_prefix('f').and_then(|d| d.parse::<u16>().ok()) {
        return (1..=24).contains(&num).then_some((0x70 + num - 1, false));
    }
    match n.as_str() {
        "enter" | "return" => vk(VK_RETURN),
        "tab" => vk(VK_TAB),
        "esc" | "escape" => vk(VK_ESCAPE),
        "backspace" => vk(VK_BACK),
        "space" => vk(0x20),
        "delete" | "del" => ext(0x2E),
        "insert" | "ins" => ext(0x2D),
        "home" => ext(0x24),
        "end" => ext(0x23),
        "pageup" => ext(0x21),
        "pagedown" => ext(0x22),
        "up" => ext(0x26),
        "down" => ext(0x28),
        "left" => ext(0x25),
        "right" => ext(0x27),
        "minus" => vk(0xBD),
        "plus" | "equals" => vk(0xBB),
        "comma" => vk(0xBC),
        "period" => vk(0xBE),
        "slash" => vk(0xBF),
        _ => None,
    }
}

fn modifier(name: &str) -> Option<u16> {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(VK_CONTROL),
        "shift" => Some(VK_SHIFT),
        "alt" => Some(VK_MENU),
        "win" | "windows" | "meta" => Some(VK_LWIN),
        _ => None,
    }
}

/// Why a keyboard request was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    #[error(
        "\"{0}\" is not a key this tool knows. Use names like ctrl+shift+s, ctrl+1, f5, enter, up."
    )]
    Unknown(String),
    #[error("a chord needs exactly one key that is not a modifier, like ctrl+s")]
    OneKey,
    #[error("the text is empty")]
    Empty,
    #[error("the text is longer than {MAX_TEXT_CHARS} characters")]
    TooLong,
    #[error("the text holds a control character this tool does not type (only line breaks, tabs, Escape and Backspace)")]
    Control,
}

/// Plans a chord: modifiers down 25 to 50 ms apart, the key held 60 to 110
/// ms, released in reverse order. A chord of modifiers only (`ctrl`) is
/// refused.
pub fn chord(text: &str, rng: &mut Rng) -> Result<Vec<KeyStep>, KeyError> {
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        // "ctrl++" means ctrl and the plus key.
        if text.trim().ends_with("++") {
            let head = &text.trim()[..text.trim().len() - 2];
            return chord(&format!("{head}+plus"), rng);
        }
        return Err(KeyError::Unknown(text.to_string()));
    }
    let mut mods = Vec::new();
    let mut key = None;
    for p in &parts {
        if let Some(m) = modifier(p) {
            if !mods.contains(&m) {
                mods.push(m);
            }
        } else if let Some(k) = named(p) {
            if key.replace(k).is_some() {
                return Err(KeyError::OneKey);
            }
        } else {
            return Err(KeyError::Unknown((*p).to_string()));
        }
    }
    let Some((vk, extended)) = key else {
        return Err(KeyError::OneKey);
    };
    let mut out = Vec::new();
    for (i, m) in mods.iter().enumerate() {
        out.push(KeyStep {
            delay_ms: if i == 0 { 0 } else { rng.range_u64(25, 50) },
            key: Key::Vk {
                vk: *m,
                extended: false,
            },
            down: true,
        });
    }
    let k = Key::Vk { vk, extended };
    out.push(KeyStep {
        delay_ms: if mods.is_empty() {
            0
        } else {
            rng.range_u64(30, 60)
        },
        key: k,
        down: true,
    });
    out.push(KeyStep {
        delay_ms: rng.range_u64(60, 110),
        key: k,
        down: false,
    });
    for m in mods.iter().rev() {
        out.push(KeyStep {
            delay_ms: rng.range_u64(20, 45),
            key: Key::Vk {
                vk: *m,
                extended: false,
            },
            down: false,
        });
    }
    Ok(out)
}

/// Plans typed text: one character every 60 to 140 ms, each held 25 to 55
/// ms. `\n` is Enter, `\t` Tab, `\u{1b}` Escape, `\u{8}` Backspace; any
/// other control character refuses the whole text.
pub fn type_text(text: &str, rng: &mut Rng) -> Result<Vec<KeyStep>, KeyError> {
    if text.is_empty() {
        return Err(KeyError::Empty);
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(KeyError::TooLong);
    }
    let text = text.replace("\r\n", "\n");
    let mut out = Vec::new();
    let mut first = true;
    for c in text.chars() {
        let keys: Vec<Key> = match c {
            '\n' => vec![Key::Vk {
                vk: VK_RETURN,
                extended: false,
            }],
            '\t' => vec![Key::Vk {
                vk: VK_TAB,
                extended: false,
            }],
            '\u{1b}' => vec![Key::Vk {
                vk: VK_ESCAPE,
                extended: false,
            }],
            '\u{8}' => vec![Key::Vk {
                vk: VK_BACK,
                extended: false,
            }],
            c if c.is_control() => return Err(KeyError::Control),
            c => {
                let mut buf = [0u16; 2];
                c.encode_utf16(&mut buf)
                    .iter()
                    .map(|u| Key::Unicode(*u))
                    .collect()
            }
        };
        let gap = if first { 0 } else { rng.range_u64(60, 140) };
        first = false;
        // Both halves of a surrogate pair go down together, then up.
        for (i, k) in keys.iter().enumerate() {
            out.push(KeyStep {
                delay_ms: if i == 0 { gap } else { 0 },
                key: *k,
                down: true,
            });
        }
        let hold = rng.range_u64(25, 55);
        for (i, k) in keys.iter().enumerate() {
            out.push(KeyStep {
                delay_ms: if i == 0 { hold } else { 0 },
                key: *k,
                down: false,
            });
        }
    }
    Ok(out)
}

/// Total time of a plan.
pub fn plan_ms(steps: &[KeyStep]) -> u64 {
    steps.iter().map(|s| s.delay_ms).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vk(v: u16) -> Key {
        Key::Vk {
            vk: v,
            extended: false,
        }
    }

    #[test]
    fn chords_press_modifiers_first_and_release_in_reverse() {
        let mut r = Rng::new(1);
        let s = chord("Ctrl+Shift+S", &mut r).expect("chord");
        let keys: Vec<(Key, bool)> = s.iter().map(|k| (k.key, k.down)).collect();
        assert_eq!(
            keys,
            vec![
                (vk(VK_CONTROL), true),
                (vk(VK_SHIFT), true),
                (vk(0x53), true),
                (vk(0x53), false),
                (vk(VK_SHIFT), false),
                (vk(VK_CONTROL), false)
            ]
        );
        let hold = s[3].delay_ms;
        assert!((60..=110).contains(&hold), "{hold}");
        assert_eq!(chord("ctrl+1", &mut r).expect("c")[1].key, vk(0x31));
        assert_eq!(chord("f5", &mut r).expect("c").len(), 2);
        assert_eq!(chord("alt+f24", &mut r).expect("c")[1].key, vk(0x87));
        assert_eq!(
            chord("up", &mut r).expect("c")[0].key,
            Key::Vk {
                vk: 0x26,
                extended: true
            }
        );
        assert_eq!(chord("ctrl++", &mut r).expect("c")[1].key, vk(0xBB));
        assert_eq!(chord("ctrl", &mut r), Err(KeyError::OneKey));
        assert_eq!(chord("ctrl+a+b", &mut r), Err(KeyError::OneKey));
        assert!(matches!(
            chord("ctrl+hyper", &mut r),
            Err(KeyError::Unknown(_))
        ));
        assert!(matches!(chord("f25", &mut r), Err(KeyError::Unknown(_))));
        assert!(matches!(chord("", &mut r), Err(KeyError::Unknown(_))));
    }

    #[test]
    fn typing_has_a_human_cadence_and_real_special_keys() {
        let mut r = Rng::new(9);
        let s = type_text("25.4\n", &mut r).expect("type");
        let downs: Vec<Key> = s.iter().filter(|k| k.down).map(|k| k.key).collect();
        assert_eq!(
            downs,
            vec![
                Key::Unicode(u16::from(b'2')),
                Key::Unicode(u16::from(b'5')),
                Key::Unicode(u16::from(b'.')),
                Key::Unicode(u16::from(b'4')),
                vk(VK_RETURN)
            ]
        );
        for st in s.iter().filter(|k| k.down).skip(1) {
            assert!((60..=140).contains(&st.delay_ms), "{}", st.delay_ms);
        }
        // A character outside the basic plane is a surrogate pair.
        let s = type_text("😀", &mut r).expect("emoji");
        assert_eq!(s.len(), 4);
        assert_eq!(s[0].key, Key::Unicode(0xD83D));
        assert_eq!(s[1].key, Key::Unicode(0xDE00));
        assert_eq!(type_text("", &mut r), Err(KeyError::Empty));
        assert_eq!(type_text("a\u{7}b", &mut r), Err(KeyError::Control));
        assert_eq!(
            type_text(&"a".repeat(MAX_TEXT_CHARS + 1), &mut r),
            Err(KeyError::TooLong)
        );
        let t = type_text("x\u{8}\u{1b}\r\n", &mut r).expect("specials");
        let d: Vec<Key> = t.iter().filter(|k| k.down).map(|k| k.key).collect();
        assert_eq!(
            d,
            vec![Key::Unicode(120), vk(VK_BACK), vk(VK_ESCAPE), vk(VK_RETURN)]
        );
        assert!(plan_ms(&t) > 0);
    }
}
