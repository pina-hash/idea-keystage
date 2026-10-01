//! Settings (ARCHITECTURE.md section 7), stored as JSON in the Tauri app
//! config directory by the app shell. Every field has a serde default so an
//! old or partial `settings.json` still loads, and [`Settings::from_json`]
//! goes further: a section that cannot be read falls back to its defaults
//! instead of failing the whole file.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::import::UserShortcut;
use crate::keys;
use crate::profile::{Profile, ProfileDisplay, GENERAL_ID, SOLIDWORKS_ID};
use crate::protocol::{limits, CustomTheme, OverlayConfig, SolidworksMode, DEFAULT_PORT, THEMES};
use crate::rules::DisplayRules;
use crate::serde_util::{u16_lenient, u32_lenient};
use crate::synth;

/// Current settings file version.
pub const SETTINGS_VERSION: u32 = 1;

/// A starting list of process names that suppress everything while they are
/// in the foreground: common password managers and Windows credential UI.
/// This is a starting list chosen for convenience, not sourced data; users
/// add and remove entries in settings. The password field guard is separate,
/// always on, and has no switch.
pub const DEFAULT_EXCLUDED_APPS: &[&str] = &[
    "1Password.exe",
    "Bitwarden.exe",
    "KeePass.exe",
    "KeePassXC.exe",
    "Enpass.exe",
    "NordPass.exe",
    "Dashlane.exe",
    "RoboForm.exe",
    "keeperpasswordmanager.exe",
    "CredentialUIBroker.exe",
    "consent.exe",
    "LogonUI.exe",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostSettings {
    /// Host A, the native overlay window.
    pub native_overlay: bool,
    /// Host A draws nothing while an OBS Browser Source is connected.
    pub hide_native_when_obs: bool,
    /// Monitor device name (`\\.\DISPLAY1`); `None` is the primary monitor.
    pub monitor: Option<String>,
    /// Keep Host A out of screen captures (Windows 10 2004 and later).
    pub exclude_from_capture: bool,
}

impl Default for HostSettings {
    fn default() -> Self {
        HostSettings {
            native_overlay: true,
            hide_native_when_obs: true,
            monitor: None,
            exclude_from_capture: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    #[serde(deserialize_with = "u16_lenient")]
    pub port: u16,
}

impl Default for ServerSettings {
    fn default() -> Self {
        ServerSettings { port: DEFAULT_PORT }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileSettings {
    pub solidworks_mode: SolidworksMode,
    /// The user's own profiles.
    pub user: Vec<Profile>,
    /// Shortcuts imported from the user's own SolidWorks.
    pub solidworks_user_shortcuts: Vec<UserShortcut>,
    /// Overrides of the SolidWorks profile's `display` block (for example
    /// `typed_values: false`); empty keeps the profile's own values.
    pub solidworks_display: ProfileDisplay,
}

impl ProfileSettings {
    /// The SolidWorks profile as used at runtime: the built-in data, the
    /// user's imported shortcuts placed first, and the display overrides.
    pub fn solidworks_profile(&self, builtin: &Profile) -> Profile {
        let mut p = builtin.merged_with_user(&self.solidworks_user_shortcuts);
        let o = &self.solidworks_display;
        if o.mode.is_some() {
            p.display.mode = o.mode;
        }
        if o.typed_values.is_some() {
            p.display.typed_values = o.typed_values;
        }
        if o.mouse_captions.is_some() {
            p.display.mouse_captions = o.mouse_captions;
        }
        p
    }

    /// Every profile in matching order: the built-in SolidWorks profile
    /// (with the user's imports), the user's profiles, then the general
    /// fallback. A user profile with a built-in id is ignored.
    pub fn all_profiles(&self, builtin_solidworks: &Profile) -> Vec<Arc<Profile>> {
        let mut out = vec![Arc::new(self.solidworks_profile(builtin_solidworks))];
        for p in &self.user {
            if p.id != SOLIDWORKS_ID && p.id != GENERAL_ID && !p.id.trim().is_empty() {
                out.push(Arc::new(p.clone()));
            }
        }
        out.push(Arc::new(Profile::general()));
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacySettings {
    /// Process names, compared case-insensitively; `.exe` may be omitted.
    pub excluded_apps: Vec<String>,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        PrivacySettings {
            excluded_apps: DEFAULT_EXCLUDED_APPS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

impl PrivacySettings {
    /// True if `exe` (a file name or a full path) is excluded.
    pub fn is_excluded(&self, exe: &str) -> bool {
        let name = crate::profile::exe_file_name(exe);
        if name.is_empty() {
            return false;
        }
        let stem = strip_exe(name);
        self.excluded_apps.iter().any(|e| {
            let e = e.trim();
            e.eq_ignore_ascii_case(name) || strip_exe(e).eq_ignore_ascii_case(stem)
        })
    }
}

fn strip_exe(s: &str) -> &str {
    let lower = s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe");
    if lower {
        &s[..s.len() - 4]
    } else {
        s
    }
}

/// Global hotkeys, as accelerator strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub pause: String,
    pub toggle_keyboard: String,
    pub toggle_solidworks: String,
    pub chapter_marker: String,
    pub session_toggle: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys {
            pause: "Ctrl+Alt+Shift+P".into(),
            toggle_keyboard: "Ctrl+Alt+Shift+K".into(),
            toggle_solidworks: "Ctrl+Alt+Shift+S".into(),
            chapter_marker: "Ctrl+Alt+Shift+M".into(),
            session_toggle: "Ctrl+Alt+Shift+L".into(),
        }
    }
}

impl Hotkeys {
    /// `(field name, accelerator)` pairs.
    pub fn entries(&self) -> [(&'static str, &str); 5] {
        [
            ("pause", self.pause.as_str()),
            ("toggle_keyboard", self.toggle_keyboard.as_str()),
            ("toggle_solidworks", self.toggle_solidworks.as_str()),
            ("chapter_marker", self.chapter_marker.as_str()),
            ("session_toggle", self.session_toggle.as_str()),
        ]
    }

    fn field_mut(&mut self, name: &str) -> &mut String {
        match name {
            "pause" => &mut self.pause,
            "toggle_keyboard" => &mut self.toggle_keyboard,
            "toggle_solidworks" => &mut self.toggle_solidworks,
            "chapter_marker" => &mut self.chapter_marker,
            _ => &mut self.session_toggle,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    pub enabled: bool,
    /// A pack id from [`synth::PACKS`].
    pub pack: String,
    /// 0 to 1.
    pub volume: f32,
}

impl Default for SoundSettings {
    fn default() -> Self {
        SoundSettings {
            enabled: false,
            pack: synth::DEFAULT_PACK.into(),
            volume: 0.6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObsSettings {
    pub enabled: bool,
    pub host: String,
    #[serde(deserialize_with = "u16_lenient")]
    pub port: u16,
    /// Base64 DPAPI (CurrentUser) blob. Never plain text.
    pub password_dpapi: Option<String>,
    /// Start a Session Log when OBS starts recording.
    pub auto_session: bool,
}

impl Default for ObsSettings {
    fn default() -> Self {
        ObsSettings {
            enabled: false,
            host: "127.0.0.1".into(),
            port: 4455,
            password_dpapi: None,
            auto_session: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionSettings {
    /// Where Session Logs are saved; `None` is the app's own folder.
    pub folder: Option<String>,
    pub autosave: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        SessionSettings {
            folder: None,
            autosave: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupSettings {
    pub start_with_windows: bool,
    /// Open the settings window when the app starts. The first run always opens it.
    pub show_settings: bool,
}

impl Default for StartupSettings {
    fn default() -> Self {
        StartupSettings {
            start_with_windows: false,
            show_settings: true,
        }
    }
}

/// Everything the app stores.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(deserialize_with = "u32_lenient")]
    pub version: u32,
    pub first_run_done: bool,
    pub host: HostSettings,
    pub server: ServerSettings,
    pub overlay: OverlayConfig,
    /// General profile rules; `ShortcutsOnly` by default.
    pub display: DisplayRules,
    pub profiles: ProfileSettings,
    pub privacy: PrivacySettings,
    pub hotkeys: Hotkeys,
    pub sound: SoundSettings,
    pub obs: ObsSettings,
    pub session: SessionSettings,
    pub startup: StartupSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SETTINGS_VERSION,
            first_run_done: false,
            host: HostSettings::default(),
            server: ServerSettings::default(),
            overlay: OverlayConfig::default(),
            display: DisplayRules::default(),
            profiles: ProfileSettings::default(),
            privacy: PrivacySettings::default(),
            hotkeys: Hotkeys::default(),
            sound: SoundSettings::default(),
            obs: ObsSettings::default(),
            session: SessionSettings::default(),
            startup: StartupSettings::default(),
        }
    }
}

/// One thing that was fixed while loading or validating settings. Messages
/// name settings fields only, never typed content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsIssue {
    /// Dotted path, e.g. `overlay.scale`.
    pub field: String,
    pub message: String,
}

fn issue(field: impl Into<String>, message: impl Into<String>) -> SettingsIssue {
    SettingsIssue {
        field: field.into(),
        message: message.into(),
    }
}

fn is_function_key(k: &str) -> bool {
    k.len() > 1 && k.starts_with('F') && k[1..].chars().all(|c| c.is_ascii_digit())
}

fn is_hex_color(s: &str) -> bool {
    let Some(h) = s.strip_prefix('#') else {
        return false;
    };
    matches!(h.len(), 3 | 4 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
}

fn clamp_f64(
    issues: &mut Vec<SettingsIssue>,
    field: &str,
    v: &mut f64,
    (lo, hi): (f64, f64),
    default: f64,
) {
    if !v.is_finite() {
        issues.push(issue(field, format!("was not a number; set to {default}")));
        *v = default;
    } else if *v < lo || *v > hi {
        let nv = v.clamp(lo, hi);
        issues.push(issue(field, format!("{v} was out of range; set to {nv}")));
        *v = nv;
    }
}

fn clamp_u64(issues: &mut Vec<SettingsIssue>, field: &str, v: &mut u64, (lo, hi): (u64, u64)) {
    if *v < lo || *v > hi {
        let nv = (*v).clamp(lo, hi);
        issues.push(issue(field, format!("{v} was out of range; set to {nv}")));
        *v = nv;
    }
}

impl Settings {
    /// Reads settings JSON. A section that cannot be read keeps its
    /// defaults and is reported; the result is always validated. Invalid
    /// JSON gives the defaults and one issue.
    pub fn from_json(s: &str) -> (Settings, Vec<SettingsIssue>) {
        let mut issues = Vec::new();
        let value: serde_json::Value = match serde_json::from_str(s) {
            Ok(v) => v,
            Err(e) => {
                issues.push(issue(
                    "",
                    format!("the settings file could not be read ({e}); defaults were used"),
                ));
                let mut d = Settings::default();
                issues.extend(d.validate());
                return (d, issues);
            }
        };
        let Some(obj) = value.as_object() else {
            issues.push(issue(
                "",
                "the settings file is not an object; defaults were used",
            ));
            let mut d = Settings::default();
            issues.extend(d.validate());
            return (d, issues);
        };
        let mut st = Settings::default();
        macro_rules! section {
            ($name:literal, $field:ident) => {
                if let Some(v) = obj.get($name) {
                    match serde_json::from_value(v.clone()) {
                        Ok(parsed) => st.$field = parsed,
                        Err(e) => issues.push(issue(
                            $name,
                            format!("could not be read ({e}); defaults were used"),
                        )),
                    }
                }
            };
        }
        section!("version", version);
        section!("first_run_done", first_run_done);
        section!("host", host);
        section!("server", server);
        section!("overlay", overlay);
        section!("display", display);
        section!("profiles", profiles);
        section!("privacy", privacy);
        section!("hotkeys", hotkeys);
        section!("sound", sound);
        section!("obs", obs);
        section!("session", session);
        section!("startup", startup);
        issues.extend(st.validate());
        (st, issues)
    }

    /// Pretty JSON for the settings file.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"))
    }

    /// Fixes everything out of range or unreadable, and returns what changed.
    pub fn validate(&mut self) -> Vec<SettingsIssue> {
        let mut out = Vec::new();
        if self.version != SETTINGS_VERSION {
            out.push(issue(
                "version",
                format!(
                    "version {} was read as version {SETTINGS_VERSION}",
                    self.version
                ),
            ));
            self.version = SETTINGS_VERSION;
        }
        if let Some(m) = &self.host.monitor {
            let t = m.trim();
            self.host.monitor = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            };
        }
        if self.server.port == 0 {
            out.push(issue(
                "server.port",
                format!("0 is not a port; set to {DEFAULT_PORT}"),
            ));
            self.server.port = DEFAULT_PORT;
        }
        self.validate_overlay(&mut out);
        for r in self.display.validate() {
            out.push(issue(format!("display.{}", r.field), r.message));
        }
        self.validate_profiles(&mut out);
        self.validate_privacy(&mut out);
        self.validate_hotkeys(&mut out);
        if !self.sound.volume.is_finite() || !(0.0..=1.0).contains(&self.sound.volume) {
            let nv = if self.sound.volume.is_finite() {
                self.sound.volume.clamp(0.0, 1.0)
            } else {
                0.6
            };
            out.push(issue(
                "sound.volume",
                format!("was out of range; set to {nv}"),
            ));
            self.sound.volume = nv;
        }
        if synth::pack(&self.sound.pack).is_none() {
            out.push(issue(
                "sound.pack",
                format!(
                    "\"{}\" is not a sound pack; set to {}",
                    self.sound.pack,
                    synth::DEFAULT_PACK
                ),
            ));
            self.sound.pack = synth::DEFAULT_PACK.into();
        }
        let host = self.obs.host.trim().to_string();
        if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c == '/') {
            out.push(issue("obs.host", "was not a host name; set to 127.0.0.1"));
            self.obs.host = "127.0.0.1".into();
        } else {
            self.obs.host = host;
        }
        if self.obs.port == 0 {
            out.push(issue("obs.port", "0 is not a port; set to 4455"));
            self.obs.port = 4455;
        }
        if let Some(blob) = &self.obs.password_dpapi {
            let t = blob.trim();
            let looks_base64 = !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'));
            if !looks_base64 {
                out.push(issue(
                    "obs.password_dpapi",
                    "was not a stored password; cleared",
                ));
                self.obs.password_dpapi = None;
            }
        }
        if let Some(f) = &self.session.folder {
            let t = f.trim();
            self.session.folder = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            };
        }
        out
    }

    fn validate_overlay(&mut self, out: &mut Vec<SettingsIssue>) {
        let o = &mut self.overlay;
        if !THEMES.contains(&o.theme.as_str()) {
            out.push(issue(
                "overlay.theme",
                format!("\"{}\" is not a theme; set to idea", o.theme),
            ));
            o.theme = "idea".into();
        }
        if let Some(c) = &mut o.custom {
            let d = CustomTheme::default();
            for (name, value, default) in [
                ("accent", &mut c.accent, d.accent),
                ("face", &mut c.face, d.face),
                ("ink", &mut c.ink, d.ink),
                ("glow", &mut c.glow, d.glow),
            ] {
                let t = value.trim().to_string();
                if is_hex_color(&t) {
                    *value = t;
                } else {
                    out.push(issue(
                        format!("overlay.custom.{name}"),
                        format!("was not a color; set to {default}"),
                    ));
                    *value = default;
                }
            }
        }
        if o.theme == "custom" && o.custom.is_none() {
            o.custom = Some(CustomTheme::default());
        }
        clamp_f64(out, "overlay.scale", &mut o.scale, limits::SCALE, 1.0);
        let c = &mut o.callouts;
        clamp_f64(
            out,
            "overlay.callouts.offset_x",
            &mut c.offset_x,
            limits::OFFSET,
            0.0,
        );
        clamp_f64(
            out,
            "overlay.callouts.offset_y",
            &mut c.offset_y,
            limits::OFFSET,
            96.0,
        );
        let (lo, hi) = limits::MAX_STACK;
        if c.max_stack < lo || c.max_stack > hi {
            let nv = c.max_stack.clamp(lo, hi);
            out.push(issue(
                "overlay.callouts.max_stack",
                format!("{} was out of range; set to {nv}", c.max_stack),
            ));
            c.max_stack = nv;
        }
        clamp_u64(
            out,
            "overlay.callouts.linger_ms",
            &mut c.linger_ms,
            limits::LINGER_MS,
        );
        clamp_u64(
            out,
            "overlay.callouts.fade_ms",
            &mut c.fade_ms,
            limits::FADE_MS,
        );
        let k = &mut o.keyboard;
        clamp_f64(
            out,
            "overlay.keyboard.offset_x",
            &mut k.offset_x,
            limits::OFFSET,
            32.0,
        );
        clamp_f64(
            out,
            "overlay.keyboard.offset_y",
            &mut k.offset_y,
            limits::OFFSET,
            32.0,
        );
        clamp_u64(
            out,
            "overlay.keyboard.decay_ms",
            &mut k.decay_ms,
            limits::DECAY_MS,
        );
        let m = &mut o.mouse;
        clamp_f64(
            out,
            "overlay.mouse.offset_x",
            &mut m.offset_x,
            limits::OFFSET,
            32.0,
        );
        clamp_f64(
            out,
            "overlay.mouse.offset_y",
            &mut m.offset_y,
            limits::OFFSET,
            32.0,
        );
        let cu = &mut o.cursor;
        clamp_f64(
            out,
            "overlay.cursor.size",
            &mut cu.size,
            limits::CURSOR_SIZE,
            44.0,
        );
        clamp_f64(
            out,
            "overlay.cursor.opacity",
            &mut cu.opacity,
            (0.0, 1.0),
            0.85,
        );
        let color = cu.color.trim().to_string();
        if is_hex_color(&color) {
            cu.color = color;
        } else {
            let d = crate::protocol::CursorConfig::default().color;
            out.push(issue(
                "overlay.cursor.color",
                format!("was not a color; set to {d}"),
            ));
            cu.color = d;
        }
    }

    fn validate_profiles(&mut self, out: &mut Vec<SettingsIssue>) {
        let mut seen: Vec<String> = Vec::new();
        let mut kept = Vec::new();
        for (i, p) in std::mem::take(&mut self.profiles.user)
            .into_iter()
            .enumerate()
        {
            let id = p.id.trim().to_string();
            if id.is_empty() {
                out.push(issue(format!("profiles.user[{i}]"), "has no id; removed"));
                continue;
            }
            if id == SOLIDWORKS_ID || id == GENERAL_ID {
                out.push(issue(
                    format!("profiles.user[{i}]"),
                    format!("uses the built-in id \"{id}\"; removed"),
                ));
                continue;
            }
            if seen.contains(&id) {
                out.push(issue(
                    format!("profiles.user[{i}]"),
                    format!("repeats the id \"{id}\"; removed"),
                ));
                continue;
            }
            seen.push(id);
            kept.push(p);
        }
        self.profiles.user = kept;
        let before = self.profiles.solidworks_user_shortcuts.len();
        self.profiles.solidworks_user_shortcuts.retain(|u| {
            !u.command.trim().is_empty()
                && !u.keys.is_empty()
                && u.keys.iter().all(|k| keys::is_canonical(k))
        });
        let dropped = before - self.profiles.solidworks_user_shortcuts.len();
        if dropped > 0 {
            out.push(issue(
                "profiles.solidworks_user_shortcuts",
                format!(
                    "{dropped} unreadable entr{} removed",
                    if dropped == 1 { "y was" } else { "ies were" }
                ),
            ));
        }
        for u in &mut self.profiles.solidworks_user_shortcuts {
            u.source = crate::profile::USER_IMPORT_SOURCE.into();
        }
    }

    fn validate_privacy(&mut self, out: &mut Vec<SettingsIssue>) {
        let mut kept: Vec<String> = Vec::new();
        for e in std::mem::take(&mut self.privacy.excluded_apps) {
            let t = e.trim().to_string();
            if t.is_empty() {
                continue;
            }
            if t.contains(['\\', '/']) {
                let name = crate::profile::exe_file_name(&t).to_string();
                out.push(issue(
                    "privacy.excluded_apps",
                    format!("\"{t}\" was shortened to \"{name}\""),
                ));
                if !name.is_empty() && !kept.iter().any(|k| k.eq_ignore_ascii_case(&name)) {
                    kept.push(name);
                }
                continue;
            }
            if !kept.iter().any(|k| k.eq_ignore_ascii_case(&t)) {
                kept.push(t);
            }
        }
        self.privacy.excluded_apps = kept;
    }

    fn validate_hotkeys(&mut self, out: &mut Vec<SettingsIssue>) {
        let defaults = Hotkeys::default();
        let mut used: Vec<String> = Vec::new();
        let names: Vec<&'static str> = self.hotkeys.entries().iter().map(|(n, _)| *n).collect();
        for name in names {
            let raw = self.hotkeys.field_mut(name).trim().to_string();
            let default = defaults
                .entries()
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            let parsed = keys::parse_accelerator(&raw);
            let canon = match parsed {
                Ok(a) if !(a.mods.has_command() || is_function_key(&a.key)) => {
                    out.push(issue(
                        format!("hotkeys.{name}"),
                        format!("\"{raw}\" needs Ctrl, Alt or Win; set to {default}"),
                    ));
                    default.clone()
                }
                Ok(a) if a.to_global_shortcut().is_none() => {
                    out.push(issue(
                        format!("hotkeys.{name}"),
                        format!("\"{raw}\" cannot be a global hotkey; set to {default}"),
                    ));
                    default.clone()
                }
                Ok(a) => a.to_string(),
                Err(e) => {
                    out.push(issue(
                        format!("hotkeys.{name}"),
                        format!("\"{raw}\" could not be read ({e}); set to {default}"),
                    ));
                    default.clone()
                }
            };
            if used.contains(&canon) {
                out.push(issue(
                    format!("hotkeys.{name}"),
                    format!("{canon} is already used by another hotkey"),
                ));
            }
            used.push(canon.clone());
            *self.hotkeys.field_mut(name) = canon;
        }
    }

    /// The general display rules with the app's own hotkeys added to
    /// `hidden_combos`, ready for [`crate::Processor::set_rules`].
    pub fn display_rules(&self) -> DisplayRules {
        let mut r = self.display.clone();
        for (_, accel) in self.hotkeys.entries() {
            if let Ok(a) = keys::parse_accelerator(accel) {
                let s = a.to_string();
                if !r.hidden_combos.contains(&s) {
                    r.hidden_combos.push(s);
                }
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::TypingMode;

    #[test]
    fn defaults_follow_section_7() {
        let s = Settings::default();
        assert_eq!(s.version, 1);
        assert_eq!(s.server.port, 47820);
        assert_eq!(s.display.typing, TypingMode::ShortcutsOnly);
        assert_eq!(s.profiles.solidworks_mode, SolidworksMode::Auto);
        assert_eq!(s.hotkeys.pause, "Ctrl+Alt+Shift+P");
        assert_eq!(s.hotkeys.toggle_keyboard, "Ctrl+Alt+Shift+K");
        assert_eq!(s.hotkeys.toggle_solidworks, "Ctrl+Alt+Shift+S");
        assert_eq!(s.hotkeys.chapter_marker, "Ctrl+Alt+Shift+M");
        assert_eq!(s.hotkeys.session_toggle, "Ctrl+Alt+Shift+L");
        assert!(!s.sound.enabled);
        assert!(!s.obs.enabled);
        assert_eq!(s.obs.host, "127.0.0.1");
        assert_eq!(s.obs.port, 4455);
        assert!(s.obs.auto_session);
        assert!(s.session.autosave);
        assert!(!s.startup.start_with_windows);
        assert!(!s.privacy.excluded_apps.is_empty());
        let mut d = Settings::default();
        assert!(d.validate().is_empty(), "the defaults are valid");
    }

    #[test]
    fn empty_and_partial_files_load() {
        let (s, issues) = Settings::from_json("{}");
        assert_eq!(s, Settings::default());
        assert!(issues.is_empty(), "{issues:?}");
        let (s, issues) =
            Settings::from_json(r#"{"server":{"port":50000},"overlay":{"scale":1.5}}"#);
        assert_eq!(s.server.port, 50000);
        assert_eq!(s.overlay.scale, 1.5);
        assert_eq!(s.overlay.callouts.linger_ms, 2200);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn a_bad_section_keeps_the_rest() {
        let (s, issues) = Settings::from_json(
            r#"{"server":{"port":"nope"},"sound":{"enabled":true},"first_run_done":true}"#,
        );
        assert_eq!(s.server.port, 47820);
        assert!(s.sound.enabled);
        assert!(s.first_run_done);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "server");
        let (s, issues) = Settings::from_json("not json");
        assert_eq!(s, Settings::default());
        assert_eq!(issues.len(), 1);
        let (_, issues) = Settings::from_json("[1,2]");
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn float_numbers_from_the_settings_window() {
        let (s, issues) = Settings::from_json(
            r#"{"overlay":{"callouts":{"linger_ms":2500.0,"max_stack":5.0}},"display":{"repeat_window_ms":800.4},"server":{"port":47821.0}}"#,
        );
        assert_eq!(s.overlay.callouts.linger_ms, 2500);
        assert_eq!(s.overlay.callouts.max_stack, 5);
        assert_eq!(s.display.repeat_window_ms, 800);
        assert_eq!(s.server.port, 47821);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn validation_clamps() {
        let mut s = Settings::default();
        s.overlay.scale = 20.0;
        s.overlay.theme = "neon".into();
        s.overlay.cursor.color = "red".into();
        s.overlay.callouts.max_stack = 0;
        s.sound.volume = 3.0;
        s.sound.pack = "missing".into();
        s.server.port = 0;
        s.obs.host = "  ".into();
        s.obs.password_dpapi = Some("not base64 !".into());
        s.version = 7;
        let issues = s.validate();
        assert_eq!(s.overlay.scale, 4.0);
        assert_eq!(s.overlay.theme, "idea");
        assert_eq!(s.overlay.cursor.color, "#78b870");
        assert_eq!(s.overlay.callouts.max_stack, 1);
        assert_eq!(s.sound.volume, 1.0);
        assert_eq!(s.sound.pack, "soft-linear");
        assert_eq!(s.server.port, 47820);
        assert_eq!(s.obs.host, "127.0.0.1");
        assert_eq!(s.obs.password_dpapi, None);
        assert_eq!(s.version, 1);
        assert_eq!(issues.len(), 10, "{issues:?}");
        let mut c = Settings::default();
        c.overlay.theme = "custom".into();
        assert!(c.validate().is_empty());
        assert!(c.overlay.custom.is_some(), "a custom theme gets colors");
    }

    #[test]
    fn hotkey_validation() {
        let mut s = Settings::default();
        s.hotkeys.pause = "ctrl + alt + shift + p".into();
        s.hotkeys.toggle_keyboard = "K".into();
        s.hotkeys.chapter_marker = "Ctrl+Banana".into();
        let issues = s.validate();
        assert_eq!(s.hotkeys.pause, "Ctrl+Alt+Shift+P");
        assert_eq!(s.hotkeys.toggle_keyboard, "Ctrl+Alt+Shift+K");
        assert_eq!(s.hotkeys.chapter_marker, "Ctrl+Alt+Shift+M");
        assert_eq!(issues.len(), 2, "{issues:?}");
        let mut d = Settings::default();
        d.hotkeys.toggle_keyboard = d.hotkeys.pause.clone();
        let issues = d.validate();
        assert!(issues.iter().any(|i| i.message.contains("already used")));
        let mut f = Settings::default();
        f.hotkeys.pause = "F9".into();
        assert!(f.validate().is_empty(), "a function key alone is allowed");
    }

    #[test]
    fn display_rules_hide_the_hotkeys() {
        let s = Settings::default();
        let r = s.display_rules();
        assert_eq!(r.hidden_combos.len(), 5);
        assert!(r.hidden_combos.contains(&"Ctrl+Alt+Shift+P".to_string()));
    }

    #[test]
    fn excluded_apps() {
        let mut s = Settings::default();
        s.privacy.excluded_apps = vec![
            " KeePass.exe ".into(),
            "keepass.EXE".into(),
            "".into(),
            "C:\\Tools\\Vault.exe".into(),
            "Secrets".into(),
        ];
        let issues = s.validate();
        assert_eq!(
            s.privacy.excluded_apps,
            vec!["KeePass.exe", "Vault.exe", "Secrets"]
        );
        assert_eq!(issues.len(), 1);
        assert!(s.privacy.is_excluded("keepass.exe"));
        assert!(s
            .privacy
            .is_excluded("C:\\Program Files\\KeePass\\KEEPASS.EXE"));
        assert!(s.privacy.is_excluded("secrets.exe"), ".exe may be omitted");
        assert!(!s.privacy.is_excluded("notepad.exe"));
        assert!(!s.privacy.is_excluded(""));
    }

    #[test]
    fn user_profiles_and_imports_are_checked() {
        let mut s = Settings::default();
        s.profiles.user = vec![
            Profile {
                id: "mine".into(),
                ..Profile::default()
            },
            Profile {
                id: "mine".into(),
                ..Profile::default()
            },
            Profile {
                id: "solidworks".into(),
                ..Profile::default()
            },
            Profile::default(),
        ];
        s.profiles.solidworks_user_shortcuts = vec![
            UserShortcut {
                keys: vec!["Ctrl".into(), "B".into()],
                command: "Fixture Command One".into(),
                ..UserShortcut::default()
            },
            UserShortcut {
                keys: vec!["Ctrl".into(), "Banana".into()],
                command: "Fixture Command Two".into(),
                ..UserShortcut::default()
            },
            UserShortcut {
                keys: vec!["Tab".into()],
                command: " ".into(),
                ..UserShortcut::default()
            },
        ];
        let issues = s.validate();
        assert_eq!(s.profiles.user.len(), 1);
        assert_eq!(s.profiles.solidworks_user_shortcuts.len(), 1);
        assert_eq!(issues.len(), 4, "{issues:?}");
    }

    #[test]
    fn solidworks_profile_overrides() {
        let builtin = Profile {
            id: SOLIDWORKS_ID.into(),
            display: ProfileDisplay {
                mode: Some(TypingMode::All),
                typed_values: Some(true),
                mouse_captions: Some(true),
            },
            ..Profile::default()
        };
        let mut ps = ProfileSettings::default();
        ps.solidworks_display.typed_values = Some(false);
        ps.solidworks_user_shortcuts.push(UserShortcut {
            keys: vec!["Ctrl".into(), "B".into()],
            command: "Fixture Command One".into(),
            ..UserShortcut::default()
        });
        let p = ps.solidworks_profile(&builtin);
        assert_eq!(p.display.typed_values, Some(false));
        assert_eq!(p.display.mode, Some(TypingMode::All));
        assert_eq!(p.shortcuts.len(), 1);
        ps.user.push(Profile {
            id: "mine".into(),
            ..Profile::default()
        });
        let all = ps.all_profiles(&builtin);
        assert_eq!(
            all.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["solidworks", "mine", "general"]
        );
    }

    #[test]
    fn round_trip() {
        let mut s = Settings {
            first_run_done: true,
            ..Settings::default()
        };
        s.profiles.solidworks_mode = SolidworksMode::On;
        let json = s.to_json_pretty();
        let (back, issues) = Settings::from_json(&json);
        assert_eq!(back, s);
        assert!(issues.is_empty());
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn show_settings_defaults_to_true_when_missing() {
        let (settings, _issues) = Settings::from_json(r#"{"startup":{"start_with_windows":true}}"#);
        assert!(settings.startup.start_with_windows);
        assert!(settings.startup.show_settings);
        let (settings, _issues) = Settings::from_json(r#"{"startup":{"show_settings":false}}"#);
        assert!(!settings.startup.show_settings);
    }
}
