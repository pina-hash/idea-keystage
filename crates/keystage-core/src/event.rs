//! Input and display event types (ARCHITECTURE.md sections 3 and 4).
//!
//! Input events come from the platform layer (the Windows hooks) or from tests.
//! Display events are what the [`crate::Processor`] decides to show; they map
//! one to one onto wire messages through [`DisplayEvent::to_server_msg`].

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::protocol::ServerMsg;

/// A physical key: the set 1 scan code plus the extended (E0 prefix) flag, as
/// the low-level keyboard hook reports them (`KBDLLHOOKSTRUCT.scanCode` and
/// `LLKHF_EXTENDED`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhysKey {
    pub scan: u16,
    pub extended: bool,
}

impl PhysKey {
    pub const fn new(scan: u16, extended: bool) -> Self {
        PhysKey { scan, extended }
    }

    /// Wire id used by the protocol and the keyboard visual: `sc:1E`, or
    /// `sc:E01D` for an extended key. Scan codes above 0xFF (the AltGr fake
    /// Ctrl uses 0x21D) are written in full: `sc:21D`.
    pub fn wire_id(&self) -> String {
        if self.extended {
            format!("sc:E0{:02X}", self.scan)
        } else {
            format!("sc:{:02X}", self.scan)
        }
    }

    /// Parses a wire id written by [`PhysKey::wire_id`]. Four hex digits that
    /// start with `E0` mean an extended key.
    pub fn parse_wire_id(s: &str) -> Option<PhysKey> {
        let hex = s.strip_prefix("sc:")?;
        if hex.is_empty() || hex.len() > 4 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let upper = hex.to_ascii_uppercase();
        if upper.len() == 4 && upper.starts_with("E0") {
            let scan = u16::from_str_radix(&upper[2..], 16).ok()?;
            return Some(PhysKey::new(scan, true));
        }
        let scan = u16::from_str_radix(&upper, 16).ok()?;
        Some(PhysKey::new(scan, false))
    }
}

impl fmt::Display for PhysKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire_id())
    }
}

/// Which of a left/right pair of modifier keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

/// The four modifier families. AltGr is reported as its own key name
/// (`"AltGr"`), not as a `Mod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mod {
    Ctrl,
    Shift,
    Alt,
    Win,
}

impl Mod {
    /// Canonical key name (`"Ctrl"`, `"Shift"`, `"Alt"`, `"Win"`).
    pub fn name(self) -> &'static str {
        match self {
            Mod::Ctrl => "Ctrl",
            Mod::Shift => "Shift",
            Mod::Alt => "Alt",
            Mod::Win => "Win",
        }
    }
}

/// One keyboard event as the low-level hook saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyInput {
    /// Virtual-key code (`KBDLLHOOKSTRUCT.vkCode`, 1 to 254).
    pub vk: u16,
    pub phys: PhysKey,
    pub down: bool,
    /// `LLKHF_INJECTED`.
    pub injected: bool,
    pub t_ms: u64,
}

/// Mouse buttons. `X1` and `X2` are the side buttons (`XBUTTON1`, `XBUTTON2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl MouseButton {
    pub const ALL: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::X1,
        MouseButton::X2,
    ];

    pub(crate) fn index(self) -> usize {
        match self {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::X1 => 3,
            MouseButton::X2 => 4,
        }
    }

    /// Display word used in mouse chips (`"Middle"`).
    pub fn word(self) -> &'static str {
        match self {
            MouseButton::Left => "Left",
            MouseButton::Right => "Right",
            MouseButton::Middle => "Middle",
            MouseButton::X1 => "X1",
            MouseButton::X2 => "X2",
        }
    }
}

/// Everything the platform layer feeds into the core.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputEvent {
    Key(KeyInput),
    Button {
        button: MouseButton,
        down: bool,
        x: i32,
        y: i32,
        injected: bool,
        t_ms: u64,
    },
    /// Raw `WHEEL_DELTA` units: +120 is one notch up (dy) or right (dx).
    /// High resolution wheels send smaller steps, which the core accumulates.
    Wheel {
        dx: i32,
        dy: i32,
        x: i32,
        y: i32,
        t_ms: u64,
    },
    /// Physical screen pixels (per-monitor DPI aware v2).
    Move {
        x: i32,
        y: i32,
        t_ms: u64,
    },
    /// The modifiers that are really down, from `GetAsyncKeyState`, sent on
    /// foreground change, unlock, resume, and whenever
    /// [`crate::Processor::needs_resync`] asks for it.
    ModifierResync {
        held: Vec<(Mod, Side)>,
        t_ms: u64,
    },
}

impl InputEvent {
    pub fn t_ms(&self) -> u64 {
        match self {
            InputEvent::Key(k) => k.t_ms,
            InputEvent::Button { t_ms, .. }
            | InputEvent::Wheel { t_ms, .. }
            | InputEvent::Move { t_ms, .. }
            | InputEvent::ModifierResync { t_ms, .. } => *t_ms,
        }
    }
}

/// The chosen monitor's rectangle in physical virtual-screen pixels (may be
/// negative on multi-monitor setups). Used to normalize mouse positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl MonitorRect {
    /// Normalizes a point to this rectangle: 0..1 inside, outside values
    /// mean the pointer is on another monitor.
    pub fn normalize(&self, x: i32, y: i32) -> (f32, f32) {
        let w = self.width.max(1) as f64;
        let h = self.height.max(1) as f64;
        let nx = (f64::from(x) - f64::from(self.x)) / w;
        let ny = (f64::from(y) - f64::from(self.y)) / h;
        (nx as f32, ny as f32)
    }
}

/// Why nothing is shown right now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "app", rename_all = "snake_case")]
pub enum Suppress {
    /// A password field has the keyboard focus.
    Password,
    /// An excluded app is in the foreground. The name stays inside the app;
    /// the wire only ever says `"excluded"`.
    ExcludedApp(String),
    /// The user paused Keystage.
    Paused,
}

/// What a callout shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CalloutKind {
    /// Modifiers plus a key, or several modifiers tapped together.
    Combo,
    /// One key on its own.
    Key,
    /// A run of typed characters.
    Text,
    /// Several steps in one callout (`Alt, F, C`).
    Chord,
    /// A mouse action (`Ctrl + Middle drag`).
    Mouse,
    /// Typing that is not shown (`Typing`), with no key identity.
    Masked,
}

/// What a chip is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChipKind {
    Modifier,
    Key,
    Mouse,
}

/// One keycap-like element of a callout.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chip {
    /// Canonical key name (section 3), or a mouse action id such as
    /// `MiddleDrag` for mouse chips.
    pub key: String,
    /// Display label (from the active keyboard layout for character keys).
    pub label: String,
    pub kind: ChipKind,
}

impl Chip {
    pub fn new(key: impl Into<String>, label: impl Into<String>, kind: ChipKind) -> Self {
        Chip {
            key: key.into(),
            label: label.into(),
            kind,
        }
    }
}

/// A callout, upserted by `id`: a later message with the same id replaces it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Callout {
    pub id: u64,
    pub kind: CalloutKind,
    /// At least one step; a plain combo has one step. Text and masked
    /// callouts have one step that is empty or holds the closing key
    /// (`Enter`, `Tab`).
    pub steps: Vec<Vec<Chip>>,
    /// The typed text, for text runs only.
    pub text: Option<String>,
    /// 1 means no badge.
    pub count: u32,
    pub caption: Option<String>,
    pub context: Option<String>,
    pub profile: Option<String>,
    pub t_ms: u64,
    pub updated_ms: u64,
}

impl Callout {
    /// Plain text form used by exports and tests: `Ctrl + Shift + S`,
    /// `Alt, F, C`, `Ctrl + Z x4`, `25.4 Enter`, `Typing`.
    pub fn text_form(&self) -> String {
        let step_text = |step: &Vec<Chip>| -> String {
            step.iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>()
                .join(" + ")
        };
        let mut s = match self.kind {
            CalloutKind::Text | CalloutKind::Masked => {
                let mut base = match self.kind {
                    CalloutKind::Text => self.text.clone().unwrap_or_default(),
                    _ => "Typing".to_string(),
                };
                let trailing: Vec<String> = self
                    .steps
                    .iter()
                    .filter(|s| !s.is_empty())
                    .map(step_text)
                    .collect();
                for t in trailing {
                    if !base.is_empty() {
                        base.push(' ');
                    }
                    base.push_str(&t);
                }
                base
            }
            _ => self
                .steps
                .iter()
                .filter(|s| !s.is_empty())
                .map(step_text)
                .collect::<Vec<_>>()
                .join(", "),
        };
        if self.count > 1 {
            s.push_str(&format!(" x{}", self.count));
        }
        s
    }
}

/// What the processor decided to show.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayEvent {
    /// Create or update the callout with this id.
    Callout(Callout),
    /// A key went down or up (for the keyboard visual). Masked keys never
    /// produce this event.
    Key {
        phys: String,
        key: String,
        down: bool,
        modifier: Option<Mod>,
        t_ms: u64,
    },
    /// x, y normalized to the chosen monitor (0..1, may fall outside).
    Button {
        button: MouseButton,
        down: bool,
        x: f32,
        y: f32,
        t_ms: u64,
    },
    Wheel {
        dx: i32,
        dy: i32,
        x: f32,
        y: f32,
        t_ms: u64,
    },
    Move {
        x: f32,
        y: f32,
        t_ms: u64,
    },
    /// Every lit key is released (suppression began or ended, resync).
    ModifiersReleased {
        t_ms: u64,
    },
}

impl DisplayEvent {
    /// The wire message for this event (section 5).
    pub fn to_server_msg(&self) -> ServerMsg {
        match self {
            DisplayEvent::Callout(c) => ServerMsg::Callout { callout: c.clone() },
            DisplayEvent::Key {
                phys,
                key,
                down,
                modifier,
                t_ms,
            } => ServerMsg::Key {
                phys: phys.clone(),
                key: key.clone(),
                down: *down,
                modifier: *modifier,
                t_ms: *t_ms,
            },
            DisplayEvent::Button {
                button,
                down,
                x,
                y,
                t_ms,
            } => ServerMsg::Button {
                button: *button,
                down: *down,
                x: *x,
                y: *y,
                t_ms: *t_ms,
            },
            DisplayEvent::Wheel { dx, dy, x, y, t_ms } => ServerMsg::Wheel {
                dx: *dx,
                dy: *dy,
                x: *x,
                y: *y,
                t_ms: *t_ms,
            },
            DisplayEvent::Move { x, y, t_ms } => ServerMsg::Move {
                x: *x,
                y: *y,
                t_ms: *t_ms,
            },
            DisplayEvent::ModifiersReleased { t_ms } => ServerMsg::ReleaseAll { t_ms: *t_ms },
        }
    }

    /// The wire JSON for this event.
    pub fn to_json(&self) -> String {
        self.to_server_msg().to_json()
    }

    /// The callout, if this event carries one.
    pub fn callout(&self) -> Option<&Callout> {
        match self {
            DisplayEvent::Callout(c) => Some(c),
            _ => None,
        }
    }
}

impl From<&DisplayEvent> for ServerMsg {
    fn from(ev: &DisplayEvent) -> Self {
        ev.to_server_msg()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_ids_round_trip() {
        assert_eq!(PhysKey::new(0x1E, false).wire_id(), "sc:1E");
        assert_eq!(PhysKey::new(0x1D, true).wire_id(), "sc:E01D");
        assert_eq!(PhysKey::new(0x02, false).wire_id(), "sc:02");
        assert_eq!(PhysKey::new(0x21D, false).wire_id(), "sc:21D");
        for p in [
            PhysKey::new(0x1E, false),
            PhysKey::new(0x1D, true),
            PhysKey::new(0x5B, true),
            PhysKey::new(0x21D, false),
            PhysKey::new(0x00, false),
        ] {
            assert_eq!(PhysKey::parse_wire_id(&p.wire_id()), Some(p));
        }
        assert_eq!(
            PhysKey::parse_wire_id("sc:e01d"),
            Some(PhysKey::new(0x1D, true))
        );
        assert_eq!(PhysKey::parse_wire_id("1E"), None);
        assert_eq!(PhysKey::parse_wire_id("sc:"), None);
        assert_eq!(PhysKey::parse_wire_id("sc:XYZ"), None);
        assert_eq!(PhysKey::parse_wire_id("sc:12345"), None);
    }

    #[test]
    fn monitor_normalization() {
        let m = MonitorRect {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let (x, y) = m.normalize(-960, 540);
        assert!((x - 0.5).abs() < 1e-6 && (y - 0.5).abs() < 1e-6);
        let (x, _) = m.normalize(100, 0);
        assert!(x > 1.0, "a point on another monitor falls outside 0..1");
        let zero = MonitorRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        let (x, y) = zero.normalize(5, 5);
        assert!(x.is_finite() && y.is_finite());
    }

    fn chip(key: &str, kind: ChipKind) -> Chip {
        Chip::new(key, key, kind)
    }

    fn callout(
        kind: CalloutKind,
        steps: Vec<Vec<Chip>>,
        text: Option<&str>,
        count: u32,
    ) -> Callout {
        Callout {
            id: 1,
            kind,
            steps,
            text: text.map(str::to_string),
            count,
            caption: None,
            context: None,
            profile: None,
            t_ms: 0,
            updated_ms: 0,
        }
    }

    #[test]
    fn text_forms() {
        let m = ChipKind::Modifier;
        let k = ChipKind::Key;
        let c = callout(
            CalloutKind::Combo,
            vec![vec![chip("Ctrl", m), chip("Shift", m), chip("S", k)]],
            None,
            1,
        );
        assert_eq!(c.text_form(), "Ctrl + Shift + S");
        let c = callout(
            CalloutKind::Combo,
            vec![vec![chip("Ctrl", m), chip("Z", k)]],
            None,
            4,
        );
        assert_eq!(c.text_form(), "Ctrl + Z x4");
        let c = callout(
            CalloutKind::Chord,
            vec![vec![chip("Alt", m)], vec![chip("F", k)], vec![chip("C", k)]],
            None,
            1,
        );
        assert_eq!(c.text_form(), "Alt, F, C");
        let c = callout(
            CalloutKind::Text,
            vec![vec![chip("Enter", k)]],
            Some("25.4"),
            1,
        );
        assert_eq!(c.text_form(), "25.4 Enter");
        let c = callout(CalloutKind::Text, vec![vec![]], Some("25.4"), 1);
        assert_eq!(c.text_form(), "25.4");
        let c = callout(CalloutKind::Masked, vec![vec![]], None, 1);
        assert_eq!(c.text_form(), "Typing");
        let c = callout(CalloutKind::Masked, vec![vec![chip("Tab", k)]], None, 1);
        assert_eq!(c.text_form(), "Typing Tab");
        let c = callout(
            CalloutKind::Mouse,
            vec![vec![
                chip("Ctrl", m),
                Chip::new("MiddleDrag", "Middle drag", ChipKind::Mouse),
            ]],
            None,
            1,
        );
        assert_eq!(c.text_form(), "Ctrl + Middle drag");
        let c = callout(
            CalloutKind::Mouse,
            vec![vec![Chip::new("WheelUp", "Wheel up", ChipKind::Mouse)]],
            None,
            5,
        );
        assert_eq!(c.text_form(), "Wheel up x5");
    }

    #[test]
    fn input_event_time() {
        let ev = InputEvent::Move {
            x: 1,
            y: 2,
            t_ms: 42,
        };
        assert_eq!(ev.t_ms(), 42);
        let ev = InputEvent::ModifierResync {
            held: vec![(Mod::Ctrl, Side::Left)],
            t_ms: 7,
        };
        assert_eq!(ev.t_ms(), 7);
    }
}
