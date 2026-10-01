//! Live app state and the pure builders for what the hosts and the settings
//! window see: the wire `AppState` (ARCHITECTURE.md section 5), the IPC
//! `Status` (section 12), and the tray's status line, tooltip and glyph.
//!
//! Everything here is plain data in, plain data out, so it is unit tested
//! without a Tauri runtime. Nothing here names an excluded app: the reason
//! enum only says why the overlay is blank.

use keystage_core::ipc::{HookStatus, ObsStatus, ServerStatus, SessionStatus, Status};
use keystage_core::protocol::{
    AppState, ProfileRef, RecordingState, SessionState, SolidworksMode, Suppressed, APP_VERSION,
};
use keystage_core::Settings;
use serde::Serialize;

/// Why nothing is shown right now, as the app knows it. The wire only ever
/// says `password` or `excluded` (or reports `paused` on its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reason {
    /// The user paused Keystage (hooks removed).
    Paused,
    /// A password field has the keyboard focus.
    Password,
    /// The session is locked (the lock screen asks for a password).
    Locked,
    /// An app on the excluded list is in the foreground.
    Excluded,
    /// The foreground app could not be identified after a focus change
    /// (secure desktop, protected process, access denied). Fail closed.
    Unidentified,
}

impl Reason {
    /// The wire `suppressed` value. Paused is reported through `paused`.
    pub fn wire(self) -> Option<Suppressed> {
        match self {
            Reason::Paused => None,
            Reason::Password | Reason::Locked => Some(Suppressed::Password),
            Reason::Excluded | Reason::Unidentified => Some(Suppressed::Excluded),
        }
    }

    /// Short UI words for the tray.
    pub fn words(self) -> &'static str {
        match self {
            Reason::Paused => "Paused",
            Reason::Password => "Hidden in password field",
            Reason::Locked => "Hidden while locked",
            Reason::Excluded => "Hidden for an excluded app",
            Reason::Unidentified => "Hidden, app not identified",
        }
    }
}

/// What the pipeline last decided about privacy and the profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PipelineView {
    /// The privacy reason in force, ignoring pause (password, locked,
    /// excluded, unidentified).
    pub privacy: Option<Reason>,
    pub profile: Option<ProfileRef>,
}

/// Runtime switches that are not stored in settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Live {
    pub paused: bool,
    pub keyboard_visible: bool,
    pub view: PipelineView,
}

impl Default for Live {
    fn default() -> Self {
        Live {
            paused: false,
            keyboard_visible: true,
            view: PipelineView::default(),
        }
    }
}

impl Live {
    /// The reason shown first: pause wins over the privacy reasons.
    pub fn reason(&self) -> Option<Reason> {
        if self.paused {
            Some(Reason::Paused)
        } else {
            self.view.privacy
        }
    }

    /// The wire `suppressed` value (password or excluded), also while paused,
    /// so the renderer can show the right pill when the pause ends.
    pub fn suppressed(&self) -> Option<Suppressed> {
        self.view.privacy.and_then(Reason::wire)
    }
}

/// Host A draws nothing when this is false: the native overlay is off, or
/// OBS Browser Sources are connected and the user asked to hide Host A then.
pub fn native_visible(settings: &Settings, obs_hosts: u32) -> bool {
    settings.host.native_overlay && !(settings.host.hide_native_when_obs && obs_hosts > 0)
}

/// Everything the wire `AppState` is built from.
#[derive(Clone, Debug)]
pub struct StateInputs<'a> {
    pub settings: &'a Settings,
    pub live: &'a Live,
    /// Connected clients that said they are OBS (for `hide_native_when_obs`).
    pub obs_hosts: u32,
    /// Connected OBS and browser sources (`AppState.obs_clients`).
    pub sources: u32,
    pub recording: Option<RecordingState>,
    pub session: Option<SessionState>,
}

/// The wire `AppState` (section 5).
pub fn app_state(i: &StateInputs<'_>) -> AppState {
    AppState {
        paused: i.live.paused,
        suppressed: i.live.suppressed(),
        profile: i.live.view.profile.clone(),
        solidworks_mode: i.settings.profiles.solidworks_mode,
        keyboard_visible: i.live.keyboard_visible,
        native_visible: native_visible(i.settings, i.obs_hosts),
        obs_clients: i.sources,
        recording: i.recording.clone(),
        session: i.session.clone(),
    }
}

/// The IPC `Status` (section 12).
pub fn status(
    settings: &Settings,
    live: &Live,
    server: ServerStatus,
    obs: ObsStatus,
    session: Option<SessionStatus>,
    hook: HookStatus,
) -> Status {
    Status {
        paused: live.paused,
        suppressed: live.suppressed(),
        profile: live.view.profile.clone(),
        solidworks_mode: settings.profiles.solidworks_mode,
        keyboard_visible: live.keyboard_visible,
        server,
        obs,
        session,
        hook,
        version: APP_VERSION.to_string(),
    }
}

/// `get_status` and the `keystage://status` event: the contract's `Status`
/// plus `notices`, plain sentences the settings window may show (a settings
/// file that had to be reset, a hotkey another program holds, input hooks
/// that could not start). Clients that do not know the field ignore it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StatusPayload {
    #[serde(flatten)]
    pub status: Status,
    pub notices: Vec<String>,
}

/// The tray icon's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrayGlyph {
    /// A lit keycap.
    Normal,
    /// The keycap with a pause sign instead of the lit face.
    Paused,
    /// The lit keycap with a round record badge.
    Recording,
}

/// Pause wins over recording: a recording without keystrokes is the state
/// the user most needs to notice.
pub fn tray_glyph(live: &Live, recording: Option<&RecordingState>) -> TrayGlyph {
    if live.paused {
        TrayGlyph::Paused
    } else if recording.is_some_and(|r| r.active) {
        TrayGlyph::Recording
    } else {
        TrayGlyph::Normal
    }
}

/// `HH:MM:SS` from an obs-websocket `HH:MM:SS.mmm` timecode.
pub fn short_timecode(tc: &str) -> &str {
    match tc.find('.') {
        Some(i) => &tc[..i],
        None => tc,
    }
}

/// The tray menu's first (disabled) line: state, profile, recording.
pub fn tray_status_line(live: &Live, recording: Option<&RecordingState>) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(3);
    parts.push(
        live.reason()
            .map(Reason::words)
            .unwrap_or("Active")
            .to_string(),
    );
    if let Some(p) = &live.view.profile {
        parts.push(p.name.clone());
    }
    if let Some(r) = recording.filter(|r| r.active) {
        if r.paused {
            parts.push(format!("REC paused {}", short_timecode(&r.timecode)));
        } else {
            parts.push(format!("REC {}", short_timecode(&r.timecode)));
        }
    }
    parts.join(" \u{b7} ")
}

/// The tray tooltip. Windows cuts tooltips at 127 characters.
pub fn tray_tooltip(live: &Live, recording: Option<&RecordingState>) -> String {
    let mut s = format!("IDEA Keystage\n{}", tray_status_line(live, recording));
    if s.chars().count() > 127 {
        s = s.chars().take(127).collect();
    }
    s
}

/// Words for the SolidWorks mode, as the tray menu shows them.
pub fn mode_words(mode: SolidworksMode) -> &'static str {
    match mode {
        SolidworksMode::Auto => "Auto",
        SolidworksMode::On => "On",
        SolidworksMode::Off => "Off",
    }
}

/// The next mode for the SolidWorks hotkey: Auto, On, Off, then Auto again.
pub fn next_mode(mode: SolidworksMode) -> SolidworksMode {
    match mode {
        SolidworksMode::Auto => SolidworksMode::On,
        SolidworksMode::On => SolidworksMode::Off,
        SolidworksMode::Off => SolidworksMode::Auto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::ipc::ObsState;

    fn profile(name: &str) -> ProfileRef {
        ProfileRef {
            id: name.to_lowercase(),
            name: name.into(),
        }
    }

    fn rec(active: bool, paused: bool) -> RecordingState {
        RecordingState {
            active,
            paused,
            timecode: "00:12:34.567".into(),
        }
    }

    #[test]
    fn reasons_map_to_the_wire_without_naming_apps() {
        assert_eq!(Reason::Paused.wire(), None);
        assert_eq!(Reason::Password.wire(), Some(Suppressed::Password));
        assert_eq!(Reason::Locked.wire(), Some(Suppressed::Password));
        assert_eq!(Reason::Excluded.wire(), Some(Suppressed::Excluded));
        assert_eq!(Reason::Unidentified.wire(), Some(Suppressed::Excluded));
    }

    #[test]
    fn pause_wins_but_privacy_stays_on_the_wire() {
        let mut live = Live::default();
        live.view.privacy = Some(Reason::Password);
        assert_eq!(live.reason(), Some(Reason::Password));
        live.paused = true;
        assert_eq!(live.reason(), Some(Reason::Paused));
        assert_eq!(live.suppressed(), Some(Suppressed::Password));
    }

    #[test]
    fn native_visibility_follows_the_host_settings() {
        let mut s = Settings::default();
        assert!(native_visible(&s, 0));
        assert!(!native_visible(&s, 1), "hidden while OBS sources connect");
        s.host.hide_native_when_obs = false;
        assert!(native_visible(&s, 3));
        s.host.native_overlay = false;
        assert!(!native_visible(&s, 0));
    }

    #[test]
    fn app_state_carries_every_part() {
        let mut s = Settings::default();
        s.profiles.solidworks_mode = SolidworksMode::On;
        let live = Live {
            keyboard_visible: false,
            view: PipelineView {
                privacy: Some(Reason::Excluded),
                profile: Some(profile("SolidWorks")),
            },
            ..Live::default()
        };
        let st = app_state(&StateInputs {
            settings: &s,
            live: &live,
            obs_hosts: 1,
            sources: 2,
            recording: Some(rec(true, false)),
            session: Some(SessionState {
                active: true,
                started_ms: 5,
                markers: 2,
            }),
        });
        assert!(!st.paused);
        assert_eq!(st.suppressed, Some(Suppressed::Excluded));
        assert_eq!(st.profile, Some(profile("SolidWorks")));
        assert_eq!(st.solidworks_mode, SolidworksMode::On);
        assert!(!st.keyboard_visible);
        assert!(!st.native_visible);
        assert_eq!(st.obs_clients, 2);
        assert!(st.recording.is_some());
        assert_eq!(st.session.map(|x| x.markers), Some(2));
        let json = serde_json::to_value(app_state(&StateInputs {
            settings: &s,
            live: &Live::default(),
            obs_hosts: 0,
            sources: 0,
            recording: None,
            session: None,
        }))
        .expect("serializes");
        assert_eq!(json["suppressed"], serde_json::Value::Null);
        assert_eq!(json["native_visible"], true);
    }

    #[test]
    fn status_payload_flattens_and_adds_notices() {
        let s = Settings::default();
        let st = status(
            &s,
            &Live::default(),
            ServerStatus {
                port: 47820,
                running: true,
                error: None,
                clients: 0,
            },
            ObsStatus {
                enabled: false,
                state: ObsState::Off,
                error: None,
                recording: None,
            },
            None,
            HookStatus {
                installed: true,
                dropped: 0,
            },
        );
        assert_eq!(st.version, APP_VERSION);
        let payload = StatusPayload {
            status: st,
            notices: vec!["A notice.".into()],
        };
        let v = serde_json::to_value(&payload).expect("serializes");
        assert_eq!(v["server"]["port"], 47820);
        assert_eq!(v["hook"]["installed"], true);
        assert_eq!(v["notices"][0], "A notice.");
        assert_eq!(v["solidworks_mode"], "auto");
    }

    #[test]
    fn tray_text_and_glyph() {
        let mut live = Live::default();
        live.view.profile = Some(profile("SolidWorks"));
        assert_eq!(tray_status_line(&live, None), "Active \u{b7} SolidWorks");
        assert_eq!(tray_glyph(&live, None), TrayGlyph::Normal);
        let r = rec(true, false);
        assert_eq!(
            tray_status_line(&live, Some(&r)),
            "Active \u{b7} SolidWorks \u{b7} REC 00:12:34"
        );
        assert_eq!(tray_glyph(&live, Some(&r)), TrayGlyph::Recording);
        assert_eq!(
            tray_glyph(&live, Some(&rec(false, false))),
            TrayGlyph::Normal
        );
        let p = rec(true, true);
        assert!(tray_status_line(&live, Some(&p)).ends_with("REC paused 00:12:34"));
        live.view.privacy = Some(Reason::Password);
        assert!(tray_status_line(&live, None).starts_with("Hidden in password field"));
        live.paused = true;
        assert!(tray_status_line(&live, Some(&r)).starts_with("Paused"));
        assert_eq!(tray_glyph(&live, Some(&r)), TrayGlyph::Paused);
        let tip = tray_tooltip(&live, Some(&r));
        assert!(tip.starts_with("IDEA Keystage\nPaused"));
        assert!(tip.chars().count() <= 127);
        live.view.profile = Some(profile(&"Long".repeat(60)));
        assert_eq!(tray_tooltip(&live, Some(&r)).chars().count(), 127);
    }

    #[test]
    fn copy_has_no_dashes() {
        for r in [
            Reason::Paused,
            Reason::Password,
            Reason::Locked,
            Reason::Excluded,
            Reason::Unidentified,
        ] {
            assert!(!r.words().contains('\u{2014}') && !r.words().contains('\u{2013}'));
        }
    }

    #[test]
    fn modes_cycle() {
        assert_eq!(next_mode(SolidworksMode::Auto), SolidworksMode::On);
        assert_eq!(next_mode(SolidworksMode::On), SolidworksMode::Off);
        assert_eq!(next_mode(SolidworksMode::Off), SolidworksMode::Auto);
        assert_eq!(mode_words(SolidworksMode::Auto), "Auto");
        assert_eq!(short_timecode("01:02:03.004"), "01:02:03");
        assert_eq!(short_timecode("01:02:03"), "01:02:03");
    }
}
