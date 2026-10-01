//! Test harness: drives a Processor with US keyboard events by canonical name.
#![allow(dead_code)]

use std::sync::Arc;

use keystage_core::{
    builtin, Callout, DisplayEvent, DisplayRules, InputEvent, KeyInput, LegendProvider,
    MouseButton, PhysKey, Processor, Profile, UsLegends,
};

/// `(vk, scan, extended)` for a canonical key name on a US keyboard.
/// Modifier names mean the left key; `RCtrl`, `RShift`, `RAlt`, `RWin` the right one.
pub fn spec(name: &str) -> (u16, u16, bool) {
    const LETTER_SCANS: [u16; 26] = [
        0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18,
        0x19, 0x10, 0x13, 0x1F, 0x14, 0x16, 0x2F, 0x11, 0x2D, 0x15, 0x2C,
    ];
    const NUMPAD_SCANS: [u16; 10] = [0x52, 0x4F, 0x50, 0x51, 0x4B, 0x4C, 0x4D, 0x47, 0x48, 0x49];
    if name.len() == 1 {
        let c = name.as_bytes()[0];
        if c.is_ascii_uppercase() {
            let i = (c - b'A') as usize;
            return (0x41 + i as u16, LETTER_SCANS[i], false);
        }
        if c.is_ascii_digit() {
            let d = (c - b'0') as u16;
            let scan = if d == 0 { 0x0B } else { 0x01 + d };
            return (0x30 + d, scan, false);
        }
        return match name {
            "-" => (0xBD, 0x0C, false),
            "=" => (0xBB, 0x0D, false),
            "[" => (0xDB, 0x1A, false),
            "]" => (0xDD, 0x1B, false),
            "\\" => (0xDC, 0x2B, false),
            ";" => (0xBA, 0x27, false),
            "'" => (0xDE, 0x28, false),
            "`" => (0xC0, 0x29, false),
            "," => (0xBC, 0x33, false),
            "." => (0xBE, 0x34, false),
            "/" => (0xBF, 0x35, false),
            _ => panic!("no spec for {name}"),
        };
    }
    if let Some(n) = name.strip_prefix('F') {
        if let Ok(v) = n.parse::<u16>() {
            let scan = match v {
                1..=10 => 0x3A + v,
                11 => 0x57,
                12 => 0x58,
                13..=24 => 0x64 + (v - 13),
                _ => panic!("bad F key"),
            };
            return (0x6F + v, scan, false);
        }
    }
    if let Some(n) = name.strip_prefix("Num") {
        if n.len() == 1 && n.as_bytes()[0].is_ascii_digit() {
            let d = (n.as_bytes()[0] - b'0') as usize;
            return (0x60 + d as u16, NUMPAD_SCANS[d], false);
        }
    }
    match name {
        "Ctrl" => (0xA2, 0x1D, false),
        "RCtrl" => (0xA3, 0x1D, true),
        "Shift" => (0xA0, 0x2A, false),
        "RShift" => (0xA1, 0x36, false),
        "Alt" => (0xA4, 0x38, false),
        "RAlt" => (0xA5, 0x38, true),
        "Win" => (0x5B, 0x5B, true),
        "RWin" => (0x5C, 0x5C, true),
        "Enter" => (0x0D, 0x1C, false),
        "NumEnter" => (0x0D, 0x1C, true),
        "Esc" => (0x1B, 0x01, false),
        "Tab" => (0x09, 0x0F, false),
        "Backspace" => (0x08, 0x0E, false),
        "Space" => (0x20, 0x39, false),
        "Up" => (0x26, 0x48, true),
        "Down" => (0x28, 0x50, true),
        "Left" => (0x25, 0x4B, true),
        "Right" => (0x27, 0x4D, true),
        "Home" => (0x24, 0x47, true),
        "End" => (0x23, 0x4F, true),
        "PageUp" => (0x21, 0x49, true),
        "PageDown" => (0x22, 0x51, true),
        "Insert" => (0x2D, 0x52, true),
        "Delete" => (0x2E, 0x53, true),
        "PrintScreen" => (0x2C, 0x37, true),
        "CapsLock" => (0x14, 0x3A, false),
        "Menu" => (0x5D, 0x5D, true),
        "NumHome" => (0x24, 0x47, false),
        "NumEnd" => (0x23, 0x4F, false),
        "NumUp" => (0x26, 0x48, false),
        "NumDelete" => (0x2E, 0x53, false),
        "NumClear" => (0x0C, 0x4C, false),
        "NumDecimal" => (0x6E, 0x53, false),
        "NumSubtract" => (0x6D, 0x4A, false),
        "NumAdd" => (0x6B, 0x4E, false),
        "NumMultiply" => (0x6A, 0x37, false),
        "NumDivide" => (0x6F, 0x35, true),
        "VolumeUp" => (0xAF, 0x30, true),
        "VolumeDown" => (0xAE, 0x2E, true),
        "VolumeMute" => (0xAD, 0x20, true),
        "MediaPlayPause" => (0xB3, 0x22, true),
        "MediaNext" => (0xB0, 0x19, true),
        _ => panic!("no spec for {name}"),
    }
}

pub fn key_event(name: &str, down: bool, t_ms: u64) -> InputEvent {
    let (vk, scan, extended) = spec(name);
    InputEvent::Key(KeyInput {
        vk,
        phys: PhysKey::new(scan, extended),
        down,
        injected: false,
        t_ms,
    })
}

/// A processor plus a clock that moves forward `step` ms before each event.
pub struct H {
    pub p: Processor,
    pub t: u64,
    pub step: u64,
    pub legends: Box<dyn LegendProvider>,
    /// Every event emitted so far.
    pub all: Vec<DisplayEvent>,
}

impl H {
    pub fn new(rules: DisplayRules) -> H {
        H {
            p: Processor::new(rules),
            t: 1000,
            step: 20,
            legends: Box::new(UsLegends),
            all: Vec::new(),
        }
    }

    pub fn general() -> H {
        H::new(DisplayRules::default())
    }

    /// The real SolidWorks profile from `data/profiles/solidworks.json`.
    pub fn solidworks() -> H {
        let mut h = H::general();
        h.p.set_profile(Arc::new(sw_profile()));
        h
    }

    pub fn with_profile(rules: DisplayRules, p: Profile) -> H {
        let mut h = H::new(rules);
        h.p.set_profile(Arc::new(p));
        h
    }

    fn record(&mut self, out: Vec<DisplayEvent>) -> Vec<DisplayEvent> {
        self.all.extend(out.iter().cloned());
        out
    }

    pub fn ev(&mut self, ev: InputEvent) -> Vec<DisplayEvent> {
        let out = self.p.handle(&ev, self.legends.as_ref());
        self.record(out)
    }

    /// Sends a key event at the current time plus `step`.
    pub fn key(&mut self, name: &str, down: bool) -> Vec<DisplayEvent> {
        self.t += self.step;
        let ev = key_event(name, down, self.t);
        self.ev(ev)
    }

    pub fn down(&mut self, name: &str) -> Vec<DisplayEvent> {
        self.key(name, true)
    }

    pub fn up(&mut self, name: &str) -> Vec<DisplayEvent> {
        self.key(name, false)
    }

    /// Down then up.
    pub fn tap(&mut self, name: &str) -> Vec<DisplayEvent> {
        let mut out = self.down(name);
        out.extend(self.up(name));
        out
    }

    /// Types each name in turn.
    pub fn type_keys(&mut self, names: &[&str]) -> Vec<DisplayEvent> {
        let mut out = Vec::new();
        for n in names {
            out.extend(self.tap(n));
        }
        out
    }

    /// Modifiers down, key tapped, modifiers up (reverse order).
    pub fn combo(&mut self, mods: &[&str], key: &str) -> Vec<DisplayEvent> {
        let mut out = Vec::new();
        for m in mods {
            out.extend(self.down(m));
        }
        out.extend(self.tap(key));
        for m in mods.iter().rev() {
            out.extend(self.up(m));
        }
        out
    }

    pub fn wait(&mut self, ms: u64) {
        self.t += ms;
    }

    pub fn tick(&mut self) -> Vec<DisplayEvent> {
        let out = self.p.tick(self.t);
        self.record(out)
    }

    pub fn button(&mut self, b: MouseButton, down: bool, x: i32, y: i32) -> Vec<DisplayEvent> {
        self.t += self.step;
        let ev = InputEvent::Button {
            button: b,
            down,
            x,
            y,
            injected: false,
            t_ms: self.t,
        };
        self.ev(ev)
    }

    pub fn mv(&mut self, x: i32, y: i32) -> Vec<DisplayEvent> {
        self.t += self.step;
        let ev = InputEvent::Move { x, y, t_ms: self.t };
        self.ev(ev)
    }

    pub fn wheel(&mut self, dx: i32, dy: i32) -> Vec<DisplayEvent> {
        self.t += self.step;
        let ev = InputEvent::Wheel {
            dx,
            dy,
            x: 500,
            y: 500,
            t_ms: self.t,
        };
        self.ev(ev)
    }

    /// The final state of every callout emitted so far, in id order.
    pub fn callouts(&self) -> Vec<Callout> {
        finals(&self.all)
    }
}

pub fn sw_profile() -> Profile {
    builtin::solidworks().expect("the embedded SolidWorks profile parses")
}

/// Callout events in `out`.
pub fn callouts_in(out: &[DisplayEvent]) -> Vec<Callout> {
    out.iter().filter_map(|e| e.callout().cloned()).collect()
}

/// The last callout event in `out`.
pub fn last(out: &[DisplayEvent]) -> Callout {
    callouts_in(out).pop().expect("a callout event")
}

/// The final state of each callout id, in id order.
pub fn finals(events: &[DisplayEvent]) -> Vec<Callout> {
    let mut map: std::collections::BTreeMap<u64, Callout> = std::collections::BTreeMap::new();
    for e in events {
        if let Some(c) = e.callout() {
            map.insert(c.id, c.clone());
        }
    }
    map.into_values().collect()
}

/// `(key, down)` of every key event in `out`.
pub fn key_events(out: &[DisplayEvent]) -> Vec<(String, bool)> {
    out.iter()
        .filter_map(|e| match e {
            DisplayEvent::Key { key, down, .. } => Some((key.clone(), *down)),
            _ => None,
        })
        .collect()
}

/// The JSON a host would receive for these events.
pub fn wire(out: &[DisplayEvent]) -> Vec<String> {
    out.iter().map(|e| e.to_json()).collect()
}

/// The data file, read and parsed once per test binary.
pub fn data_json() -> &'static serde_json::Value {
    static DATA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/profiles/solidworks.json"
        ))
        .expect("read data/profiles/solidworks.json");
        serde_json::from_str(&text).expect("valid JSON")
    })
}

/// Reference caption lookup straight from the data file's JSON, written
/// independently of the crate: entries whose `keys` hold the same modifiers
/// and the same non-modifier keys in order; the first with an empty context
/// wins, otherwise the first in file order.
pub fn reference_caption(keys: &[&str]) -> Option<(String, Option<String>)> {
    let v = data_json();
    let is_mod = |k: &str| matches!(k, "Ctrl" | "Shift" | "Alt" | "AltGr" | "Win");
    let split = |list: Vec<String>| {
        let mut mods: Vec<String> = list.iter().filter(|k| is_mod(k)).cloned().collect();
        mods.sort();
        let rest: Vec<String> = list.iter().filter(|k| !is_mod(k)).cloned().collect();
        (mods, rest)
    };
    let want = split(keys.iter().map(|s| s.to_string()).collect());
    let mut first: Option<(String, Option<String>)> = None;
    for s in v["shortcuts"].as_array().expect("shortcuts") {
        let ks: Vec<String> = s["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect();
        if split(ks) != want {
            continue;
        }
        let command = s["command"].as_str().unwrap().to_string();
        let context = s["context"].as_str().unwrap_or("").trim().to_string();
        if context.is_empty() {
            return Some((command, None));
        }
        if first.is_none() {
            first = Some((command, Some(context)));
        }
    }
    first
}

/// Reference mouse caption lookup from the data file, with the documented
/// rules: `unspecified` means the left button for click, double click and
/// drag; wheel entries without a direction match both directions; gestures
/// need the same direction; drag and click entries with a direction never match.
pub fn reference_mouse_caption(
    button: &str,
    action: &str,
    mods: &[&str],
    direction: Option<&str>,
) -> Option<(String, Option<String>)> {
    let v = data_json();
    let mut want_mods: Vec<String> = mods.iter().map(|s| s.to_string()).collect();
    want_mods.sort();
    let mut first: Option<(String, Option<String>)> = None;
    for m in v["mouse"].as_array().expect("mouse") {
        if m["action"].as_str() != Some(action) {
            continue;
        }
        let mut ms: Vec<String> = m["modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect();
        ms.sort();
        if ms != want_mods {
            continue;
        }
        let b = m["button"].as_str().unwrap();
        let button_ok = match action {
            "wheel" => true,
            "click" | "double_click" | "drag" => {
                b == button || (b == "unspecified" && button == "left")
            }
            _ => b == button,
        };
        if !button_ok {
            continue;
        }
        let d = m["direction"].as_str();
        let dir_ok = match action {
            "gesture" => d.is_some() && d == direction,
            "wheel" => d.is_none() || d == direction,
            _ => d.is_none(),
        };
        if !dir_ok {
            continue;
        }
        let command = m["command"].as_str().unwrap().to_string();
        let context = m["context"].as_str().unwrap_or("").trim().to_string();
        if context.is_empty() {
            return Some((command, None));
        }
        if first.is_none() {
            first = Some((command, Some(context)));
        }
    }
    first
}
