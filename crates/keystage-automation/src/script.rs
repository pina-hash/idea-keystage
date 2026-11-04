//! `perform`: a whole take as one script (docs/ARCHITECTURE.md section 14).
//! The script is read and checked here before anything moves; a script
//! with any problem is refused as a whole, with every problem named.

use serde::{Deserialize, Serialize};

use crate::keys;
use crate::motion::{Rect, Rng};

/// Most actions in one script.
pub const MAX_ACTIONS: usize = 500;
/// Longest total wait in one script.
pub const MAX_WAIT_TOTAL_MS: u64 = 30 * 60 * 1000;
/// Most points in one drag.
pub const MAX_DRAG_POINTS: usize = 200;
/// Longest `wait_idle`.
pub const MAX_IDLE_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Where to point: physical virtual-screen pixels, or fractions of a named
/// monitor (`monitor` is its device name, `\\.\DISPLAY1`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub monitor: Option<String>,
}

/// Wait until a small screen rectangle stops changing (150 ms unchanged),
/// at most `max_ms`. Without `rect`, 64 by 64 pixels around the target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Idle {
    #[serde(default)]
    pub rect: Option<Rect>,
    #[serde(default = "default_idle_max")]
    pub max_ms: u64,
}

fn default_idle_max() -> u64 {
    3_000
}

/// One action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Move {
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        monitor: Option<String>,
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    Click {
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        monitor: Option<String>,
        #[serde(default)]
        button: Button,
        #[serde(default)]
        double: bool,
        #[serde(default)]
        wait_idle: Option<Idle>,
    },
    Drag {
        path: Vec<[f64; 2]>,
        #[serde(default)]
        button: Button,
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    Scroll {
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        monitor: Option<String>,
        clicks: u32,
        direction: Direction,
    },
    Chord {
        keys: String,
    },
    Type {
        text: String,
    },
    Wait {
        ms: u64,
    },
    WaitIdle {
        #[serde(default)]
        rect: Option<Rect>,
        #[serde(default = "default_idle_max")]
        max_ms: u64,
    },
    Marker {
        name: String,
        #[serde(default)]
        step: Option<String>,
    },
    ObsStart,
    ObsStop,
}

impl Action {
    /// Where a move, click or scroll points (`None` for other actions).
    pub fn target(&self) -> Option<Target> {
        match self {
            Action::Move { x, y, monitor, .. }
            | Action::Click { x, y, monitor, .. }
            | Action::Scroll { x, y, monitor, .. } => Some(Target {
                x: *x,
                y: *y,
                monitor: monitor.clone(),
            }),
            _ => None,
        }
    }

    pub fn op(&self) -> &'static str {
        match self {
            Action::Move { .. } => "move",
            Action::Click { .. } => "click",
            Action::Drag { .. } => "drag",
            Action::Scroll { .. } => "scroll",
            Action::Chord { .. } => "chord",
            Action::Type { .. } => "type",
            Action::Wait { .. } => "wait",
            Action::WaitIdle { .. } => "wait_idle",
            Action::Marker { .. } => "marker",
            Action::ObsStart => "obs_start",
            Action::ObsStop => "obs_stop",
        }
    }

    /// True for actions that send input.
    pub fn sends_input(&self) -> bool {
        matches!(
            self,
            Action::Move { .. }
                | Action::Click { .. }
                | Action::Drag { .. }
                | Action::Scroll { .. }
                | Action::Chord { .. }
                | Action::Type { .. }
        )
    }
}

fn default_true() -> bool {
    true
}

/// The `perform` arguments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub actions: Vec<Action>,
    /// Refuse to start when another window covers the capture monitor.
    #[serde(default = "default_true")]
    pub require_clean: bool,
    /// The monitor being recorded (device name); default the one the
    /// overlay uses, else the primary.
    #[serde(default)]
    pub capture_monitor: Option<String>,
    /// Seed for the curves and the typing rhythm; random when absent.
    #[serde(default)]
    pub seed: Option<u64>,
}

/// What the screen looks like, for checking targets.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub virtual_screen: Rect,
    /// `(device name, rectangle)` of every monitor.
    pub monitors: Vec<(String, Rect)>,
}

impl Screen {
    pub fn monitor(&self, id: &str) -> Option<Rect> {
        self.monitors
            .iter()
            .find(|(m, _)| m.eq_ignore_ascii_case(id))
            .map(|(_, r)| *r)
    }

    /// The pixel a target means, or why it means none.
    pub fn resolve(&self, t: &Target) -> Result<Option<(i32, i32)>, String> {
        match (t.x, t.y, &t.monitor) {
            (None, None, None) => Ok(None),
            (Some(x), Some(y), Some(m)) => {
                let r = self
                    .monitor(m)
                    .ok_or_else(|| format!("there is no monitor named {m}"))?;
                if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                    return Err("with a monitor, x and y are fractions from 0 to 1".into());
                }
                Ok(Some(crate::motion::on_monitor(x, y, &r)))
            }
            (Some(x), Some(y), None) => {
                if !x.is_finite() || !y.is_finite() || x.fract() != 0.0 || y.fract() != 0.0 {
                    return Err("x and y are whole physical pixels".into());
                }
                let (xi, yi) = (x as i32, y as i32);
                if !self.monitors.iter().any(|(_, r)| r.contains(xi, yi)) {
                    return Err(format!("{xi}, {yi} is on no monitor"));
                }
                Ok(Some((xi, yi)))
            }
            _ => Err("give both x and y (and optionally monitor)".into()),
        }
    }
}

/// Checks a script against the screen. Returns every problem, numbered by
/// action (from 0), or nothing.
pub fn validate(s: &Script, screen: &Screen) -> Vec<String> {
    let mut out = Vec::new();
    if s.actions.is_empty() {
        out.push("the script has no actions".into());
    }
    if s.actions.len() > MAX_ACTIONS {
        out.push(format!(
            "the script has {} actions; the limit is {MAX_ACTIONS}",
            s.actions.len()
        ));
    }
    if let Some(m) = &s.capture_monitor {
        if screen.monitor(m).is_none() {
            out.push(format!("there is no monitor named {m}"));
        }
    }
    let mut waited = 0u64;
    let mut rng = Rng::new(0);
    for (i, a) in s.actions.iter().enumerate() {
        let mut bad = |m: String| out.push(format!("action {i} ({}): {m}", a.op()));
        match a {
            Action::Move { duration_ms, .. } => {
                match screen.resolve(&a.target().unwrap_or_default()) {
                    Ok(Some(_)) => {}
                    Ok(None) => bad("a move needs x and y".into()),
                    Err(e) => bad(e),
                }
                if duration_ms.is_some_and(|d| d > crate::motion::MAX_DURATION_MS) {
                    bad("duration_ms is at most 60000".into());
                }
            }
            Action::Click { wait_idle, .. } => {
                if let Err(e) = screen.resolve(&a.target().unwrap_or_default()) {
                    bad(e);
                }
                if wait_idle.as_ref().is_some_and(|w| w.max_ms > MAX_IDLE_MS) {
                    bad("wait_idle max_ms is at most 30000".into());
                }
            }
            Action::Drag {
                path, duration_ms, ..
            } => {
                if path.len() < 2 || path.len() > MAX_DRAG_POINTS {
                    bad(format!("a drag needs 2 to {MAX_DRAG_POINTS} points"));
                }
                for p in path {
                    let t = Target {
                        x: Some(p[0]),
                        y: Some(p[1]),
                        monitor: None,
                    };
                    if let Err(e) = screen.resolve(&t) {
                        bad(e);
                        break;
                    }
                }
                if duration_ms.is_some_and(|d| d > crate::motion::MAX_DURATION_MS) {
                    bad("duration_ms is at most 60000".into());
                }
            }
            Action::Scroll { clicks, .. } => {
                match screen.resolve(&a.target().unwrap_or_default()) {
                    Ok(Some(_)) => {}
                    Ok(None) => bad("a scroll needs x and y".into()),
                    Err(e) => bad(e),
                }
                if !(1..=100).contains(clicks) {
                    bad("clicks is 1 to 100".into());
                }
            }
            Action::Chord { keys: k } => {
                if let Err(e) = keys::chord(k, &mut rng) {
                    bad(e.to_string());
                }
            }
            Action::Type { text } => {
                if let Err(e) = keys::type_text(text, &mut rng) {
                    bad(e.to_string());
                }
            }
            Action::Wait { ms } => waited = waited.saturating_add(*ms),
            Action::WaitIdle { max_ms, .. } => {
                if *max_ms > MAX_IDLE_MS {
                    bad("max_ms is at most 30000".into());
                }
                waited = waited.saturating_add(*max_ms);
            }
            Action::Marker { name, step } => {
                if name.trim().is_empty() || name.chars().count() > 100 {
                    bad("a marker name is 1 to 100 characters".into());
                }
                if step.as_ref().is_some_and(|s| s.chars().count() > 64) {
                    bad("a step id is at most 64 characters".into());
                }
            }
            Action::ObsStart | Action::ObsStop => {}
        }
    }
    if waited > MAX_WAIT_TOTAL_MS {
        out.push("the script waits more than 30 minutes in all".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn screen() -> Screen {
        Screen {
            virtual_screen: Rect {
                x: -1920,
                y: 0,
                width: 3840,
                height: 1080,
            },
            monitors: vec![
                (
                    r"\\.\DISPLAY1".into(),
                    Rect {
                        x: 0,
                        y: 0,
                        width: 1920,
                        height: 1080,
                    },
                ),
                (
                    r"\\.\DISPLAY2".into(),
                    Rect {
                        x: -1920,
                        y: 0,
                        width: 1920,
                        height: 1080,
                    },
                ),
            ],
        }
    }

    fn script(v: serde_json::Value) -> Script {
        serde_json::from_value(v).expect("script")
    }

    #[test]
    fn a_whole_take_reads_and_passes() {
        let s = script(json!({
            "actions": [
                {"op": "obs_start"},
                {"op": "marker", "name": "Sketch", "step": "s1"},
                {"op": "move", "x": 960, "y": 540},
                {"op": "click", "x": 0.5, "y": 0.25, "monitor": "\\\\.\\DISPLAY2", "wait_idle": {"max_ms": 2000}},
                {"op": "drag", "path": [[100, 100], [400, 300]], "button": "middle"},
                {"op": "scroll", "x": 960, "y": 540, "clicks": 3, "direction": "down"},
                {"op": "chord", "keys": "ctrl+1"},
                {"op": "type", "text": "25.4\n"},
                {"op": "wait", "ms": 500},
                {"op": "wait_idle", "max_ms": 1500},
                {"op": "obs_stop"}
            ]
        }));
        assert!(s.require_clean, "clean by default");
        assert_eq!(validate(&s, &screen()), Vec::<String>::new());
        assert_eq!(s.actions[3].op(), "click");
        assert!(s.actions[2].sends_input() && !s.actions[1].sends_input());
        let r = screen().resolve(&Target {
            x: Some(0.5),
            y: Some(0.25),
            monitor: Some(r"\\.\display2".into()),
        });
        assert_eq!(r, Ok(Some((-960, 270))));
    }

    #[test]
    fn every_problem_is_named() {
        let s = script(json!({
            "actions": [
                {"op": "move", "x": 5000, "y": 5},
                {"op": "move", "x": 1.5, "y": 2},
                {"op": "click", "x": 0.5, "y": 2, "monitor": "\\\\.\\DISPLAY1"},
                {"op": "click", "x": 1, "monitor": "\\\\.\\DISPLAY9"},
                {"op": "drag", "path": [[1, 1]]},
                {"op": "scroll", "x": 1, "y": 1, "clicks": 0, "direction": "up"},
                {"op": "chord", "keys": "ctrl"},
                {"op": "type", "text": ""},
                {"op": "marker", "name": " "},
                {"op": "wait", "ms": 1900000}
            ],
            "capture_monitor": "nope"
        }));
        let v = validate(&s, &screen());
        assert_eq!(v.len(), 11, "{v:#?}");
        assert!(v[0].contains("no monitor named nope"));
        assert!(v[1].starts_with("action 0 (move)") && v[1].contains("on no monitor"));
        assert!(v[2].contains("whole physical pixels"));
        assert!(v.last().expect("l").contains("30 minutes"));
        let empty = script(json!({"actions": []}));
        assert_eq!(
            validate(&empty, &screen()),
            vec!["the script has no actions".to_string()]
        );
    }

    #[test]
    fn unknown_ops_and_fields_are_refused_when_read() {
        assert!(
            serde_json::from_value::<Script>(json!({"actions": [{"op": "launch_missiles"}]}))
                .is_err()
        );
        assert!(serde_json::from_value::<Script>(
            json!({"actions": [{"op": "wait", "ms": 1, "extra": 1}]})
        )
        .is_err());
        assert!(serde_json::from_value::<Script>(json!({"actions": [], "loop": true})).is_err());
    }
}
