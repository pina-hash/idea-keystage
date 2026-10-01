//! Display rules (ARCHITECTURE.md section 4). The defaults are the general
//! profile's rules; a profile's `display` block overrides some of them.

use serde::{Deserialize, Serialize};

use crate::keys::{self, Accelerator};
use crate::serde_util::{u32_lenient, u64_lenient};

/// Whether typed text is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypingMode {
    /// Printable keys with no modifier other than Shift (or AltGr) are
    /// masked. The default outside SolidWorks.
    #[default]
    ShortcutsOnly,
    /// Bare single keys are shown (the SolidWorks profile).
    All,
}

/// What the processor shows and how it groups it. Every field has a default,
/// so an old or partial settings file still loads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayRules {
    pub typing: TypingMode,
    /// With `TypingMode::All`: typed values (digits, `.`, `-`, letters that
    /// are not shortcuts) form text runs. Off: they are masked, but profile
    /// single-key shortcuts still show.
    pub typed_values: bool,
    /// `Left Ctrl` instead of `Ctrl` in labels.
    pub show_modifier_side: bool,
    /// The same combo again within this window adds to the same callout.
    #[serde(deserialize_with = "u64_lenient")]
    pub repeat_window_ms: u64,
    /// A solo Alt tap, or a profile chord leader, waits this long for the next step.
    #[serde(deserialize_with = "u64_lenient")]
    pub chord_window_ms: u64,
    /// Most steps in an Alt chord (`Alt, F, C` is 3). Keeps a quick Alt tap
    /// from showing a long run of typed letters.
    #[serde(deserialize_with = "u32_lenient")]
    pub chord_max_steps: u32,
    /// Typed characters closer than this join one text run.
    #[serde(deserialize_with = "u64_lenient")]
    pub text_run_gap_ms: u64,
    /// Mouse callouts (drags, clicks, wheel, gestures). On in the SolidWorks profile.
    pub mouse_callouts: bool,
    /// Also show left clicks, double clicks and drags that have no modifier.
    /// Off by default because every selection would make a callout.
    pub plain_left_clicks: bool,
    /// Injected (synthetic) input is shown. Off: injected key and button
    /// events are ignored completely.
    pub show_injected: bool,
    /// Accelerators that are never shown (the app's own global hotkeys).
    pub hidden_combos: Vec<String>,
    /// Movement beyond this many physical pixels while a button is down is a drag.
    #[serde(deserialize_with = "u32_lenient")]
    pub drag_threshold_px: u32,
    /// Two clicks of one button within this time ...
    #[serde(deserialize_with = "u64_lenient")]
    pub double_click_ms: u64,
    /// ... and this distance are a double click.
    #[serde(deserialize_with = "u32_lenient")]
    pub double_click_px: u32,
    /// A right drag at least this long ...
    #[serde(deserialize_with = "u32_lenient")]
    pub gesture_min_px: u32,
    /// ... released within this time is a mouse gesture candidate.
    #[serde(deserialize_with = "u64_lenient")]
    pub gesture_max_ms: u64,
    /// A modifier held longer than this without fresh evidence makes
    /// [`crate::Processor::needs_resync`] ask the platform layer for a
    /// `ModifierResync`.
    #[serde(deserialize_with = "u64_lenient")]
    pub modifier_verify_ms: u64,
    /// A modifier with no evidence for this long is released by `tick`
    /// (its key-up was lost, for example across a lock screen).
    #[serde(deserialize_with = "u64_lenient")]
    pub modifier_stale_ms: u64,
}

impl Default for DisplayRules {
    fn default() -> Self {
        DisplayRules {
            typing: TypingMode::ShortcutsOnly,
            typed_values: true,
            show_modifier_side: false,
            repeat_window_ms: 1000,
            chord_window_ms: 900,
            chord_max_steps: 4,
            text_run_gap_ms: 1500,
            mouse_callouts: false,
            plain_left_clicks: false,
            show_injected: true,
            hidden_combos: Vec::new(),
            drag_threshold_px: 4,
            double_click_ms: 500,
            double_click_px: 4,
            gesture_min_px: 40,
            gesture_max_ms: 1500,
            modifier_verify_ms: 2000,
            modifier_stale_ms: 10_000,
        }
    }
}

/// One problem found by [`DisplayRules::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleIssue {
    pub field: &'static str,
    pub message: String,
}

impl DisplayRules {
    /// Clamps out-of-range values and drops unreadable hidden combos.
    /// Returns what was changed.
    pub fn validate(&mut self) -> Vec<RuleIssue> {
        let mut issues = Vec::new();
        let mut clamp = |field: &'static str, v: &mut u64, lo: u64, hi: u64| {
            if *v < lo || *v > hi {
                let nv = (*v).clamp(lo, hi);
                issues.push(RuleIssue {
                    field,
                    message: format!("{} was out of range; set to {}", v, nv),
                });
                *v = nv;
            }
        };
        clamp("repeat_window_ms", &mut self.repeat_window_ms, 0, 10_000);
        clamp("chord_window_ms", &mut self.chord_window_ms, 0, 5_000);
        clamp("text_run_gap_ms", &mut self.text_run_gap_ms, 100, 10_000);
        clamp("double_click_ms", &mut self.double_click_ms, 100, 2_000);
        clamp("gesture_max_ms", &mut self.gesture_max_ms, 100, 10_000);
        clamp(
            "modifier_verify_ms",
            &mut self.modifier_verify_ms,
            250,
            60_000,
        );
        clamp(
            "modifier_stale_ms",
            &mut self.modifier_stale_ms,
            1_000,
            600_000,
        );
        let mut clamp32 = |field: &'static str, v: &mut u32, lo: u32, hi: u32| {
            if *v < lo || *v > hi {
                let nv = (*v).clamp(lo, hi);
                issues.push(RuleIssue {
                    field,
                    message: format!("{} was out of range; set to {}", v, nv),
                });
                *v = nv;
            }
        };
        clamp32("chord_max_steps", &mut self.chord_max_steps, 2, 8);
        clamp32("drag_threshold_px", &mut self.drag_threshold_px, 1, 200);
        clamp32("double_click_px", &mut self.double_click_px, 1, 200);
        clamp32("gesture_min_px", &mut self.gesture_min_px, 10, 2_000);
        let mut kept = Vec::new();
        for raw in std::mem::take(&mut self.hidden_combos) {
            match keys::parse_accelerator(&raw) {
                Ok(a) => {
                    let canon = a.to_string();
                    if !kept.contains(&canon) {
                        kept.push(canon);
                    }
                }
                Err(e) => issues.push(RuleIssue {
                    field: "hidden_combos",
                    message: format!("\"{raw}\" was removed: {e}"),
                }),
            }
        }
        self.hidden_combos = kept;
        issues
    }

    /// The hidden combos that parse.
    pub fn parsed_hidden_combos(&self) -> Vec<Accelerator> {
        self.hidden_combos
            .iter()
            .filter_map(|s| keys::parse_accelerator(s).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_section_4() {
        let r = DisplayRules::default();
        assert_eq!(r.typing, TypingMode::ShortcutsOnly);
        assert_eq!(r.repeat_window_ms, 1000);
        assert_eq!(r.chord_window_ms, 900);
        assert_eq!(r.text_run_gap_ms, 1500);
        assert!(!r.mouse_callouts);
        assert!(r.show_injected);
        assert_eq!(r.drag_threshold_px, 4);
        assert_eq!(r.double_click_ms, 500);
        assert_eq!(r.double_click_px, 4);
        assert_eq!(r.gesture_min_px, 40);
        assert_eq!(r.gesture_max_ms, 1500);
    }

    #[test]
    fn partial_json_loads_with_defaults() {
        let r: DisplayRules =
            serde_json::from_str(r#"{"typing":"all","repeat_window_ms":700}"#).unwrap();
        assert_eq!(r.typing, TypingMode::All);
        assert_eq!(r.repeat_window_ms, 700);
        assert_eq!(r.chord_window_ms, 900);
        let r: DisplayRules = serde_json::from_str("{}").unwrap();
        assert_eq!(r, DisplayRules::default());
        let r: DisplayRules = serde_json::from_str(r#"{"unknown_field": 3}"#).unwrap();
        assert_eq!(r, DisplayRules::default());
        let s = serde_json::to_string(&TypingMode::ShortcutsOnly).unwrap();
        assert_eq!(s, "\"shortcuts_only\"");
    }

    #[test]
    fn validation_clamps_and_cleans() {
        let mut r = DisplayRules {
            repeat_window_ms: 99_999,
            text_run_gap_ms: 1,
            chord_max_steps: 0,
            hidden_combos: vec![
                "Ctrl+Alt+Shift+P".into(),
                "ctrl+alt+shift+p".into(),
                "Ctrl+Banana".into(),
            ],
            ..DisplayRules::default()
        };
        let issues = r.validate();
        assert_eq!(r.repeat_window_ms, 10_000);
        assert_eq!(r.text_run_gap_ms, 100);
        assert_eq!(r.chord_max_steps, 2);
        assert_eq!(r.hidden_combos, vec!["Ctrl+Alt+Shift+P".to_string()]);
        assert_eq!(issues.len(), 4, "{issues:?}");
        let mut ok = DisplayRules::default();
        assert!(ok.validate().is_empty());
        assert_eq!(r.parsed_hidden_combos().len(), 1);
    }
}
