//! Profiles (ARCHITECTURE.md section 9): the serde model of
//! `data/profiles/*.json`, matching by process name, and caption lookup.
//!
//! Captions come only from profile data (the sourced SolidWorks file or the
//! user's own imports). This module never adds a caption of its own.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::event::MouseButton;
use crate::import::UserShortcut;
use crate::keys;
use crate::modifiers::{ModKey, ModSet};
use crate::protocol::{ProfileRef, SolidworksMode};
use crate::rules::{DisplayRules, TypingMode};

/// Schema id every profile file carries.
pub const PROFILE_SCHEMA: &str = "keystage.profile/1";
/// Id of the built-in SolidWorks profile.
pub const SOLIDWORKS_ID: &str = "solidworks";
/// Id of the built-in fallback profile.
pub const GENERAL_ID: &str = "general";
/// `source` value of shortcuts imported from the user's own SolidWorks.
pub const USER_IMPORT_SOURCE: &str = "user-import";

/// `match` block: which foreground processes select the profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileMatch {
    /// Executable names, compared case-insensitively (`SLDWORKS.exe`).
    pub process: Vec<String>,
}

/// `display` block: overrides of the general display rules.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileDisplay {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<TypingMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typed_values: Option<bool>,
    /// Mouse callouts with captions from the `mouse` table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_captions: Option<bool>,
}

/// `visual` block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileVisual {
    /// A theme id to switch to while the profile is active; `null` keeps the user's theme.
    pub theme: Option<String>,
}

impl ProfileVisual {
    pub fn is_default(&self) -> bool {
        self.theme.is_none()
    }
}

/// A documentation page an entry cites.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    pub id: String,
    pub url: String,
    pub title: String,
    pub version: String,
    /// Date read, `YYYY-MM-DD`.
    pub read: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One keyboard shortcut entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcut {
    pub id: String,
    /// Canonical key names. Leading modifiers plus one key is a combo;
    /// modifiers only is a modifier tap; more keys after the first are
    /// further steps (`["Alt","F","C"]` is `Alt+F`, then `C`).
    pub keys: Vec<String>,
    /// The keys as the documentation writes them.
    pub doc_keys: String,
    /// The caption.
    pub command: String,
    pub description: String,
    pub group: String,
    /// Where the shortcut applies; empty means everywhere.
    pub context: String,
    pub versions: Vec<String>,
    pub source_ids: Vec<String>,
    pub quote: String,
    /// `"user-import"` for entries from the user's own SolidWorks; absent in data files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One mouse entry. Strings are kept as they are on disk and interpreted
/// when the caption index is built.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseEntry {
    pub id: String,
    /// `left`, `right`, `middle`, `x1`, `x2`, or `unspecified` when the
    /// documentation does not name a button.
    pub button: String,
    /// `click`, `double_click`, `drag`, `wheel` or `gesture`. Other values
    /// (the data file has one `press`) are kept but never matched.
    pub action: String,
    pub modifiers: Vec<String>,
    /// For gestures and directional drags: `up`, `up_right`, `right`,
    /// `down_right`, `down`, `down_left`, `left`, `up_left`.
    pub direction: Option<String>,
    pub command: String,
    pub description: String,
    pub context: String,
    pub versions: Vec<String>,
    pub source_ids: Vec<String>,
    pub quote: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A documentation gap: something the research looked for and did not find.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gap {
    pub topic: String,
    pub url: String,
    pub reason: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A profile, exactly as stored on disk. Every field has a default so a
/// user profile can be minimal; optional blocks absent from a file stay
/// absent when it is written back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub schema: String,
    pub id: String,
    pub name: String,
    #[serde(rename = "match")]
    pub matcher: ProfileMatch,
    pub display: ProfileDisplay,
    #[serde(skip_serializing_if = "ProfileVisual::is_default")]
    pub visual: ProfileVisual,
    /// Chord leaders: after one of these combos, the next combo within the
    /// chord window joins the same callout as a second step.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chords: Vec<Vec<String>>,
    /// A note about where the data comes from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_note: Option<String>,
    /// Date the sources were read, `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<String>,
    pub sources: Vec<Source>,
    pub shortcuts: Vec<Shortcut>,
    pub mouse: Vec<MouseEntry>,
    /// Differences between documented versions, as sentences.
    pub differences: Vec<String>,
    pub gaps: Vec<Gap>,
    /// Any other top-level field, kept so a round trip is lossless.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Why a profile could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("the profile is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the profile schema \"{0}\" is not supported")]
    Schema(String),
    #[error("the profile has no id")]
    NoId,
}

impl Profile {
    /// The fallback profile: no match, no captions, general display rules.
    pub fn general() -> Profile {
        Profile {
            schema: PROFILE_SCHEMA.into(),
            id: GENERAL_ID.into(),
            name: "General".into(),
            ..Profile::default()
        }
    }

    /// Parses and checks a profile file.
    pub fn from_json(s: &str) -> Result<Profile, ProfileError> {
        let p: Profile = serde_json::from_str(s)?;
        if !p.schema.is_empty() && p.schema != PROFILE_SCHEMA {
            return Err(ProfileError::Schema(p.schema));
        }
        if p.id.trim().is_empty() {
            return Err(ProfileError::NoId);
        }
        Ok(p)
    }

    pub fn reference(&self) -> ProfileRef {
        ProfileRef {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }

    /// True if `exe` (a bare file name or a full path) is in `match.process`,
    /// ignoring case.
    pub fn matches_process(&self, exe: &str) -> bool {
        let name = exe_file_name(exe);
        !name.is_empty()
            && self
                .matcher
                .process
                .iter()
                .any(|p| p.trim().eq_ignore_ascii_case(name))
    }

    /// The display rules in force while this profile is active.
    pub fn apply_display(&self, base: &DisplayRules) -> DisplayRules {
        let mut r = base.clone();
        if let Some(m) = self.display.mode {
            r.typing = m;
        }
        if let Some(t) = self.display.typed_values {
            r.typed_values = t;
        }
        if let Some(m) = self.display.mouse_captions {
            r.mouse_callouts = m;
        }
        r
    }

    /// This profile with the user's imported shortcuts placed first (they win
    /// over the data file for the same keys), tagged `source: "user-import"`.
    pub fn merged_with_user(&self, user: &[UserShortcut]) -> Profile {
        let mut p = self.clone();
        let mut merged: Vec<Shortcut> = user
            .iter()
            .enumerate()
            .map(|(i, u)| Shortcut {
                id: format!("user-{:03}", i + 1),
                keys: u.keys.clone(),
                doc_keys: u.doc_keys.clone(),
                command: u.command.clone(),
                group: u.category.clone(),
                source: Some(USER_IMPORT_SOURCE.into()),
                ..Shortcut::default()
            })
            .collect();
        merged.extend(p.shortcuts);
        p.shortcuts = merged;
        p
    }
}

/// The file name part of a path, for process matching.
pub fn exe_file_name(exe: &str) -> &str {
    let t = exe.trim();
    t.rsplit(['\\', '/']).next().unwrap_or(t)
}

/// Picks the profile for the foreground process (section 9). `profiles`
/// holds the built-ins and the user's profiles in priority order; the
/// general profile (or `None`) is the fallback.
pub fn select_profile<'a>(
    mode: SolidworksMode,
    foreground_exe: Option<&str>,
    profiles: &'a [Arc<Profile>],
) -> Option<&'a Arc<Profile>> {
    if mode == SolidworksMode::On {
        if let Some(p) = profiles.iter().find(|p| p.id == SOLIDWORKS_ID) {
            return Some(p);
        }
    }
    if let Some(exe) = foreground_exe {
        let found = profiles.iter().find(|p| {
            !(mode == SolidworksMode::Off && p.id == SOLIDWORKS_ID) && p.matches_process(exe)
        });
        if found.is_some() {
            return found;
        }
    }
    profiles.iter().find(|p| p.id == GENERAL_ID)
}

/// One step of a key sequence: modifiers plus an optional key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Step {
    pub mods: ModSet,
    pub key: Option<String>,
}

impl Step {
    pub fn new(mods: ModSet, key: Option<&str>) -> Step {
        Step {
            mods,
            key: key.map(str::to_string),
        }
    }
}

/// Converts a profile `keys` array into steps. Returns `None` if a name is
/// not canonical.
pub fn keys_to_steps(keys_list: &[String]) -> Option<Vec<Step>> {
    let mut steps = Vec::new();
    let mut cur = Step {
        mods: ModSet::EMPTY,
        key: None,
    };
    for k in keys_list {
        let k = k.trim();
        if let Some(m) = ModKey::from_name(k) {
            if cur.key.is_some() {
                steps.push(std::mem::replace(
                    &mut cur,
                    Step {
                        mods: ModSet::EMPTY,
                        key: None,
                    },
                ));
            }
            cur.mods.insert(m);
        } else if keys::is_canonical(k) {
            if cur.key.is_some() {
                steps.push(std::mem::replace(
                    &mut cur,
                    Step {
                        mods: ModSet::EMPTY,
                        key: None,
                    },
                ));
            }
            cur.key = Some(k.to_string());
        } else {
            return None;
        }
    }
    if cur.key.is_some() || !cur.mods.is_empty() {
        steps.push(cur);
    }
    if steps.is_empty() {
        None
    } else {
        Some(steps)
    }
}

/// Normalizes observed steps for lookup: a leading modifier-only step (an
/// Alt tap) is merged into the next step, because `Alt, F` reaches the same
/// menu as `Alt+F`.
pub fn normalize_steps(steps: &[Step]) -> Vec<Step> {
    if steps.len() >= 2 && steps[0].key.is_none() && steps[1].mods.is_empty() {
        let mut out = Vec::with_capacity(steps.len() - 1);
        out.push(Step {
            mods: steps[0].mods,
            key: steps[1].key.clone(),
        });
        out.extend(steps[2..].iter().cloned());
        out
    } else {
        steps.to_vec()
    }
}

/// A caption found for an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caption {
    pub command: String,
    /// `None` when the entry applies everywhere.
    pub context: Option<String>,
    /// True if it came from the user's own import.
    pub user: bool,
}

/// What a mouse callout is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseAction {
    Click,
    DoubleClick,
    Drag,
    Wheel,
    Gesture,
}

impl MouseAction {
    pub fn parse(s: &str) -> Option<MouseAction> {
        Some(match s.trim() {
            "click" => MouseAction::Click,
            "double_click" => MouseAction::DoubleClick,
            "drag" => MouseAction::Drag,
            "wheel" => MouseAction::Wheel,
            "gesture" => MouseAction::Gesture,
            _ => return None,
        })
    }
}

/// One of the eight gesture directions, or a wheel direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Direction> {
        Some(match s.trim() {
            "up" => Direction::Up,
            "up_right" => Direction::UpRight,
            "right" => Direction::Right,
            "down_right" => Direction::DownRight,
            "down" => Direction::Down,
            "down_left" => Direction::DownLeft,
            "left" => Direction::Left,
            "up_left" => Direction::UpLeft,
            _ => return None,
        })
    }

    /// The data file's spelling.
    pub fn id(self) -> &'static str {
        match self {
            Direction::Up => "up",
            Direction::UpRight => "up_right",
            Direction::Right => "right",
            Direction::DownRight => "down_right",
            Direction::Down => "down",
            Direction::DownLeft => "down_left",
            Direction::Left => "left",
            Direction::UpLeft => "up_left",
        }
    }

    /// Words for labels (`up-right`).
    pub fn words(self) -> &'static str {
        match self {
            Direction::Up => "up",
            Direction::UpRight => "up-right",
            Direction::Right => "right",
            Direction::DownRight => "down-right",
            Direction::Down => "down",
            Direction::DownLeft => "down-left",
            Direction::Left => "left",
            Direction::UpLeft => "up-left",
        }
    }

    /// PascalCase for chip keys (`UpRight`).
    pub fn pascal(self) -> &'static str {
        match self {
            Direction::Up => "Up",
            Direction::UpRight => "UpRight",
            Direction::Right => "Right",
            Direction::DownRight => "DownRight",
            Direction::Down => "Down",
            Direction::DownLeft => "DownLeft",
            Direction::Left => "Left",
            Direction::UpLeft => "UpLeft",
        }
    }

    /// Classifies a screen vector (y grows downward) into eight 45 degree sectors.
    pub fn from_vector(dx: f64, dy: f64) -> Direction {
        let angle = (-dy).atan2(dx).to_degrees();
        let a = if angle < 0.0 { angle + 360.0 } else { angle };
        let sector = (((a + 22.5) / 45.0).floor() as i64).rem_euclid(8);
        match sector {
            0 => Direction::Right,
            1 => Direction::UpRight,
            2 => Direction::Up,
            3 => Direction::UpLeft,
            4 => Direction::Left,
            5 => Direction::DownLeft,
            6 => Direction::Down,
            _ => Direction::DownRight,
        }
    }
}

/// A mouse input to caption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseQuery {
    pub button: MouseButton,
    pub action: MouseAction,
    pub mods: ModSet,
    /// Gesture direction, or `Up`/`Down`/`Left`/`Right` for the wheel.
    pub direction: Option<Direction>,
}

#[derive(Clone, Debug)]
enum ButtonSpec {
    Exact(MouseButton),
    Unspecified,
}

#[derive(Clone, Debug)]
struct MouseRule {
    button: ButtonSpec,
    action: MouseAction,
    mods: ModSet,
    direction: Option<Direction>,
    caption: Caption,
}

/// Lookup tables built once per profile switch.
#[derive(Clone, Debug, Default)]
pub struct CaptionIndex {
    profile_id: Option<String>,
    seqs: HashMap<Vec<Step>, Caption>,
    /// Proper prefixes of multi-step entries: after one of these, the next
    /// step may complete a sequence.
    prefixes: HashSet<Vec<Step>>,
    /// Explicit chord leaders from the profile's `chords`.
    leaders: HashSet<Step>,
    mouse: Vec<MouseRule>,
    /// Entries skipped because a key name was not canonical.
    pub skipped: usize,
}

impl CaptionIndex {
    /// Builds the index. `user` entries win over the profile's for the same
    /// keys. Among profile entries for the same keys, the first one with an
    /// empty `context` wins; otherwise the first in file order.
    pub fn build(profile: &Profile, user: &[UserShortcut]) -> CaptionIndex {
        let mut idx = CaptionIndex {
            profile_id: Some(profile.id.clone()),
            ..Default::default()
        };
        let consider = |idx: &mut CaptionIndex, keys_list: &[String], cap: Caption| {
            let Some(steps) = keys_to_steps(keys_list) else {
                idx.skipped += 1;
                return;
            };
            for n in 1..steps.len() {
                idx.prefixes.insert(steps[..n].to_vec());
            }
            match idx.seqs.get(&steps) {
                None => {
                    idx.seqs.insert(steps, cap);
                }
                Some(existing) => {
                    let replace = !existing.user
                        && !cap.user
                        && existing.context.is_some()
                        && cap.context.is_none();
                    if replace {
                        idx.seqs.insert(steps, cap);
                    }
                }
            }
        };
        for u in user {
            let command = u.command.trim();
            if command.is_empty() {
                continue;
            }
            consider(
                &mut idx,
                &u.keys,
                Caption {
                    command: command.to_string(),
                    context: None,
                    user: true,
                },
            );
        }
        for s in &profile.shortcuts {
            let command = s.command.trim();
            if command.is_empty() {
                continue;
            }
            let user_entry = s.source.as_deref() == Some(USER_IMPORT_SOURCE);
            let context = if s.context.trim().is_empty() {
                None
            } else {
                Some(s.context.trim().to_string())
            };
            consider(
                &mut idx,
                &s.keys,
                Caption {
                    command: command.to_string(),
                    context,
                    user: user_entry,
                },
            );
        }
        for chord in &profile.chords {
            if let Some(steps) = keys_to_steps(chord) {
                if steps.len() == 1 {
                    idx.leaders.insert(steps[0].clone());
                }
            }
        }
        for m in &profile.mouse {
            let Some(action) = MouseAction::parse(&m.action) else {
                idx.skipped += 1;
                continue;
            };
            let button = match m.button.trim() {
                "left" => ButtonSpec::Exact(MouseButton::Left),
                "right" => ButtonSpec::Exact(MouseButton::Right),
                "middle" => ButtonSpec::Exact(MouseButton::Middle),
                "x1" => ButtonSpec::Exact(MouseButton::X1),
                "x2" => ButtonSpec::Exact(MouseButton::X2),
                "unspecified" | "" => ButtonSpec::Unspecified,
                _ => {
                    idx.skipped += 1;
                    continue;
                }
            };
            let Some(mods) = ModSet::from_names(m.modifiers.iter().map(|s| s.trim())) else {
                idx.skipped += 1;
                continue;
            };
            let direction = match m.direction.as_deref().map(str::trim) {
                None | Some("") => None,
                Some(d) => match Direction::parse(d) {
                    Some(d) => Some(d),
                    None => {
                        idx.skipped += 1;
                        continue;
                    }
                },
            };
            let command = m.command.trim();
            if command.is_empty() {
                continue;
            }
            let context = if m.context.trim().is_empty() {
                None
            } else {
                Some(m.context.trim().to_string())
            };
            idx.mouse.push(MouseRule {
                button,
                action,
                mods,
                direction,
                caption: Caption {
                    command: command.to_string(),
                    context,
                    user: false,
                },
            });
        }
        idx
    }

    pub fn profile_id(&self) -> Option<&str> {
        self.profile_id.as_deref()
    }

    /// The caption for a key sequence (one step for a plain combo).
    pub fn lookup(&self, steps: &[Step]) -> Option<&Caption> {
        self.seqs.get(steps)
    }

    /// True if `steps` begins a longer sequence in the profile.
    pub fn is_prefix(&self, steps: &[Step]) -> bool {
        self.prefixes.contains(steps)
    }

    /// True if the profile declares this combo as a chord leader.
    pub fn is_leader(&self, step: &Step) -> bool {
        self.leaders.contains(step)
    }

    /// The caption for a mouse input.
    ///
    /// Matching rules: modifiers must match exactly. An entry whose button is
    /// `"unspecified"` (the documentation says "click" or "drag" without
    /// naming a button) matches the left button for clicks, double clicks and
    /// drags, since that is what an unqualified click means on Windows. Wheel
    /// entries (action `"wheel"`) with no direction match both wheel
    /// directions. Gesture entries match only when their direction equals the
    /// gesture's, so a gesture is captioned only from a sourced entry for that
    /// direction. Drag and click entries with a direction are not matched,
    /// because drags are not classified by direction. Tie-break as for keys:
    /// the first entry with an empty context, otherwise the first in file order.
    pub fn lookup_mouse(&self, q: &MouseQuery) -> Option<&Caption> {
        let mut first: Option<&Caption> = None;
        for r in &self.mouse {
            if r.action != q.action || r.mods != q.mods {
                continue;
            }
            let button_ok = match (&r.button, q.action) {
                (_, MouseAction::Wheel) => true,
                (ButtonSpec::Exact(b), _) => *b == q.button,
                (
                    ButtonSpec::Unspecified,
                    MouseAction::Click | MouseAction::DoubleClick | MouseAction::Drag,
                ) => q.button == MouseButton::Left,
                (ButtonSpec::Unspecified, _) => false,
            };
            if !button_ok {
                continue;
            }
            let dir_ok = match q.action {
                MouseAction::Gesture => r.direction.is_some() && r.direction == q.direction,
                MouseAction::Wheel => r.direction.is_none() || r.direction == q.direction,
                _ => r.direction.is_none(),
            };
            if !dir_ok {
                continue;
            }
            if r.caption.context.is_none() {
                return Some(&r.caption);
            }
            if first.is_none() {
                first = Some(&r.caption);
            }
        }
        first
    }

    /// Number of distinct key sequences with a caption.
    pub fn len(&self) -> usize {
        self.seqs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seqs.is_empty() && self.mouse.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(keys: &[&str], command: &str, context: &str) -> Shortcut {
        Shortcut {
            keys: keys.iter().map(|s| s.to_string()).collect(),
            command: command.into(),
            context: context.into(),
            ..Shortcut::default()
        }
    }

    fn me(
        button: &str,
        action: &str,
        mods: &[&str],
        dir: Option<&str>,
        command: &str,
        context: &str,
    ) -> MouseEntry {
        MouseEntry {
            button: button.into(),
            action: action.into(),
            modifiers: mods.iter().map(|s| s.to_string()).collect(),
            direction: dir.map(str::to_string),
            command: command.into(),
            context: context.into(),
            ..MouseEntry::default()
        }
    }

    fn step(mods: &[&str], key: Option<&str>) -> Step {
        Step::new(ModSet::from_names(mods.iter().copied()).unwrap(), key)
    }

    #[test]
    fn keys_to_steps_shapes() {
        let s = |v: &[&str]| keys_to_steps(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(s(&["Ctrl", "8"]), Some(vec![step(&["Ctrl"], Some("8"))]));
        assert_eq!(
            s(&["Shift", "Alt"]),
            Some(vec![step(&["Shift", "Alt"], None)])
        );
        assert_eq!(
            s(&["Alt", "F", "C"]),
            Some(vec![step(&["Alt"], Some("F")), step(&[], Some("C"))])
        );
        assert_eq!(
            s(&["Ctrl", "K", "Ctrl", "C"]),
            Some(vec![step(&["Ctrl"], Some("K")), step(&["Ctrl"], Some("C"))])
        );
        assert_eq!(s(&["Space"]), Some(vec![step(&[], Some("Space"))]));
        assert_eq!(s(&["Ctrl", "Banana"]), None);
        assert_eq!(s(&[]), None);
    }

    #[test]
    fn normalization_merges_alt_tap() {
        let observed = vec![
            step(&["Alt"], None),
            step(&[], Some("F")),
            step(&[], Some("C")),
        ];
        assert_eq!(
            normalize_steps(&observed),
            vec![step(&["Alt"], Some("F")), step(&[], Some("C"))]
        );
        let combo = vec![step(&["Ctrl"], Some("K")), step(&["Ctrl"], Some("C"))];
        assert_eq!(normalize_steps(&combo), combo);
    }

    fn fixture_profile() -> Profile {
        Profile {
            schema: PROFILE_SCHEMA.into(),
            id: "fixture".into(),
            name: "Fixture".into(),
            matcher: ProfileMatch {
                process: vec!["Fixture.EXE".into()],
            },
            chords: vec![vec!["Ctrl".into(), "K".into()]],
            shortcuts: vec![
                sc(&["Space"], "Fixture space in a mode", "Some mode"),
                sc(&["Space"], "Fixture space everywhere", ""),
                sc(&["Space"], "Fixture space later", ""),
                sc(&["Tab"], "Fixture tab one", "Mode one"),
                sc(&["Tab"], "Fixture tab two", "Mode two"),
                sc(&["Alt", "F", "C"], "Fixture sequence", ""),
                sc(&["Ctrl", "Banana"], "Fixture broken", ""),
            ],
            mouse: vec![
                me("middle", "drag", &[], None, "Fixture middle drag", ""),
                me(
                    "unspecified",
                    "drag",
                    &["Ctrl"],
                    None,
                    "Fixture ctrl drag",
                    "Somewhere",
                ),
                me("middle", "wheel", &[], None, "Fixture wheel", ""),
                me(
                    "right",
                    "gesture",
                    &[],
                    Some("up"),
                    "Fixture gesture up",
                    "Sketch",
                ),
                me(
                    "right",
                    "gesture",
                    &[],
                    None,
                    "Fixture gestures in general",
                    "",
                ),
                me(
                    "left",
                    "drag",
                    &[],
                    Some("up"),
                    "Fixture directional drag",
                    "Tool",
                ),
                me(
                    "unspecified",
                    "click",
                    &["Shift"],
                    None,
                    "Fixture shift click",
                    "",
                ),
                me("sideways", "click", &[], None, "Fixture bad button", ""),
            ],
            ..Profile::default()
        }
    }

    #[test]
    fn caption_tie_break() {
        let idx = CaptionIndex::build(&fixture_profile(), &[]);
        let space = idx.lookup(&[step(&[], Some("Space"))]).unwrap();
        assert_eq!(
            space.command, "Fixture space everywhere",
            "empty context wins"
        );
        assert_eq!(space.context, None);
        let tab = idx.lookup(&[step(&[], Some("Tab"))]).unwrap();
        assert_eq!(
            tab.command, "Fixture tab one",
            "otherwise the first in file order"
        );
        assert_eq!(tab.context.as_deref(), Some("Mode one"));
        assert_eq!(
            idx.skipped, 2,
            "the bad key name and the bad button are skipped"
        );
        assert!(idx.is_prefix(&[step(&["Alt"], Some("F"))]));
        assert!(!idx.is_prefix(&[step(&["Alt"], Some("C"))]));
        assert!(idx.is_leader(&step(&["Ctrl"], Some("K"))));
        assert_eq!(
            idx.lookup(&[step(&["Alt"], Some("F")), step(&[], Some("C"))])
                .unwrap()
                .command,
            "Fixture sequence"
        );
    }

    #[test]
    fn user_imports_win() {
        let user = vec![UserShortcut {
            keys: vec!["Tab".into()],
            doc_keys: "Tab".into(),
            command: "Fixture user tab".into(),
            category: "FixtureCategory".into(),
            source: USER_IMPORT_SOURCE.into(),
        }];
        let idx = CaptionIndex::build(&fixture_profile(), &user);
        let tab = idx.lookup(&[step(&[], Some("Tab"))]).unwrap();
        assert_eq!(tab.command, "Fixture user tab");
        assert!(tab.user);
        // Same result through the merged profile.
        let merged = fixture_profile().merged_with_user(&user);
        assert_eq!(
            merged.shortcuts[0].source.as_deref(),
            Some(USER_IMPORT_SOURCE)
        );
        let idx2 = CaptionIndex::build(&merged, &[]);
        assert_eq!(
            idx2.lookup(&[step(&[], Some("Tab"))]).unwrap().command,
            "Fixture user tab"
        );
    }

    #[test]
    fn mouse_lookup_rules() {
        let idx = CaptionIndex::build(&fixture_profile(), &[]);
        let q = |button, action, mods: &[&str], direction| MouseQuery {
            button,
            action,
            mods: ModSet::from_names(mods.iter().copied()).unwrap(),
            direction,
        };
        let cap = |query| idx.lookup_mouse(&query).map(|c| c.command.clone());
        assert_eq!(
            cap(q(MouseButton::Middle, MouseAction::Drag, &[], None)).as_deref(),
            Some("Fixture middle drag")
        );
        assert_eq!(
            cap(q(MouseButton::Left, MouseAction::Drag, &["Ctrl"], None)).as_deref(),
            Some("Fixture ctrl drag")
        );
        assert_eq!(
            cap(q(MouseButton::Right, MouseAction::Drag, &["Ctrl"], None)),
            None,
            "unspecified means left"
        );
        assert_eq!(
            cap(q(MouseButton::Middle, MouseAction::Drag, &["Ctrl"], None)),
            None
        );
        assert_eq!(
            cap(q(MouseButton::Left, MouseAction::Drag, &[], None)),
            None,
            "directional drag entries do not match"
        );
        assert_eq!(
            cap(q(
                MouseButton::Middle,
                MouseAction::Wheel,
                &[],
                Some(Direction::Up)
            ))
            .as_deref(),
            Some("Fixture wheel")
        );
        assert_eq!(
            cap(q(
                MouseButton::Middle,
                MouseAction::Wheel,
                &[],
                Some(Direction::Down)
            ))
            .as_deref(),
            Some("Fixture wheel")
        );
        assert_eq!(
            cap(q(
                MouseButton::Right,
                MouseAction::Gesture,
                &[],
                Some(Direction::Up)
            ))
            .as_deref(),
            Some("Fixture gesture up")
        );
        assert_eq!(
            cap(q(
                MouseButton::Right,
                MouseAction::Gesture,
                &[],
                Some(Direction::Left)
            )),
            None,
            "no sourced entry for left"
        );
        assert_eq!(
            cap(q(MouseButton::Left, MouseAction::Click, &["Shift"], None)).as_deref(),
            Some("Fixture shift click")
        );
        assert_eq!(
            cap(q(
                MouseButton::Left,
                MouseAction::DoubleClick,
                &["Shift"],
                None
            )),
            None
        );
    }

    #[test]
    fn process_matching_and_selection() {
        let fixture = Arc::new(fixture_profile());
        assert!(fixture.matches_process("fixture.exe"));
        assert!(fixture.matches_process("C:\\Program Files\\Fixture\\FIXTURE.EXE"));
        assert!(!fixture.matches_process("other.exe"));
        assert!(!fixture.matches_process(""));
        let sw = Arc::new(Profile {
            id: SOLIDWORKS_ID.into(),
            matcher: ProfileMatch {
                process: vec!["SLDWORKS.exe".into()],
            },
            ..Profile::default()
        });
        let general = Arc::new(Profile::general());
        let all = vec![sw.clone(), fixture.clone(), general.clone()];
        let pick = |mode, exe| select_profile(mode, exe, &all).map(|p| p.id.clone());
        assert_eq!(
            pick(SolidworksMode::Auto, Some("sldworks.exe")).as_deref(),
            Some("solidworks")
        );
        assert_eq!(
            pick(SolidworksMode::Auto, Some("notepad.exe")).as_deref(),
            Some("general")
        );
        assert_eq!(pick(SolidworksMode::Auto, None).as_deref(), Some("general"));
        assert_eq!(
            pick(SolidworksMode::On, Some("notepad.exe")).as_deref(),
            Some("solidworks")
        );
        assert_eq!(
            pick(SolidworksMode::Off, Some("SLDWORKS.EXE")).as_deref(),
            Some("general")
        );
        assert_eq!(
            pick(SolidworksMode::Off, Some("fixture.exe")).as_deref(),
            Some("fixture")
        );
        assert_eq!(
            select_profile(SolidworksMode::Auto, Some("x.exe"), &[]),
            None
        );
    }

    #[test]
    fn display_overrides() {
        let p = Profile {
            display: ProfileDisplay {
                mode: Some(TypingMode::All),
                typed_values: Some(false),
                mouse_captions: Some(true),
            },
            ..Profile::default()
        };
        let r = p.apply_display(&DisplayRules::default());
        assert_eq!(r.typing, TypingMode::All);
        assert!(!r.typed_values);
        assert!(r.mouse_callouts);
        let g = Profile::general().apply_display(&DisplayRules::default());
        assert_eq!(g, DisplayRules::default());
    }

    #[test]
    fn json_round_trip_keeps_unknown_fields() {
        let src = r#"{"schema":"keystage.profile/1","id":"x","name":"X","match":{"process":["x.exe"]},"display":{"mode":"all"},"shortcuts":[{"id":"kb-1","keys":["Ctrl","B"],"command":"Fixture","note":"kept"}],"future":42}"#;
        let p = Profile::from_json(src).unwrap();
        assert_eq!(p.display.mode, Some(TypingMode::All));
        assert_eq!(p.extra.get("future"), Some(&serde_json::json!(42)));
        assert_eq!(
            p.shortcuts[0].extra.get("note"),
            Some(&serde_json::json!("kept"))
        );
        let back: serde_json::Value = serde_json::to_value(&p).unwrap();
        assert_eq!(back["future"], 42);
        assert_eq!(back["shortcuts"][0]["note"], "kept");
        assert!(
            back["shortcuts"][0].get("source").is_none(),
            "absent source stays absent"
        );
        assert!(matches!(
            Profile::from_json(r#"{"schema":"other/2","id":"x"}"#),
            Err(ProfileError::Schema(_))
        ));
        assert!(matches!(
            Profile::from_json(r#"{"schema":"keystage.profile/1"}"#),
            Err(ProfileError::NoId)
        ));
        assert!(matches!(
            Profile::from_json("not json"),
            Err(ProfileError::Json(_))
        ));
    }

    #[test]
    fn gesture_directions() {
        let d = |dx, dy| Direction::from_vector(dx, dy);
        assert_eq!(d(0.0, -50.0), Direction::Up);
        assert_eq!(d(50.0, -50.0), Direction::UpRight);
        assert_eq!(d(50.0, 0.0), Direction::Right);
        assert_eq!(d(50.0, 50.0), Direction::DownRight);
        assert_eq!(d(0.0, 50.0), Direction::Down);
        assert_eq!(d(-50.0, 50.0), Direction::DownLeft);
        assert_eq!(d(-50.0, 0.0), Direction::Left);
        assert_eq!(d(-50.0, -50.0), Direction::UpLeft);
        assert_eq!(
            d(50.0, -10.0),
            Direction::Right,
            "within 22.5 degrees of right"
        );
        assert_eq!(d(50.0, -30.0), Direction::UpRight);
        for dir in [
            Direction::Up,
            Direction::UpRight,
            Direction::Right,
            Direction::DownRight,
            Direction::Down,
            Direction::DownLeft,
            Direction::Left,
            Direction::UpLeft,
        ] {
            assert_eq!(Direction::parse(dir.id()), Some(dir));
        }
    }
}
