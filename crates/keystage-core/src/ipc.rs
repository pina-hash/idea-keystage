//! Types for the settings window IPC (ARCHITECTURE.md section 12). Only the
//! shapes live here; the app shell implements the commands. `Settings` is in
//! [`crate::settings`], the import preview in [`crate::import`].

use serde::{Deserialize, Serialize};

use crate::protocol::{ProfileRef, RecordingState, Route, SolidworksMode, Suppressed};

/// `get_status` and the `keystage://status` event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub paused: bool,
    pub suppressed: Option<Suppressed>,
    pub profile: Option<ProfileRef>,
    pub solidworks_mode: SolidworksMode,
    pub keyboard_visible: bool,
    pub server: ServerStatus,
    pub obs: ObsStatus,
    pub session: Option<SessionStatus>,
    pub hook: HookStatus,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerStatus {
    pub port: u16,
    pub running: bool,
    pub error: Option<String>,
    pub clients: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObsState {
    #[default]
    Off,
    Connecting,
    Connected,
    AuthFailed,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObsStatus {
    pub enabled: bool,
    pub state: ObsState,
    pub error: Option<String>,
    pub recording: Option<RecordingState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStatus {
    pub id: String,
    pub active: bool,
    /// ISO 8601 start time.
    pub started: String,
    pub callouts: u32,
    pub markers: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookStatus {
    pub installed: bool,
    /// Queue overflow count; should stay 0.
    pub dropped: u64,
}

/// `list_monitors` entry, physical pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monitor {
    /// Device name (`\\.\DISPLAY1`).
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// DPI scale (1.5 = 150 %).
    pub scale: f64,
    pub primary: bool,
}

/// `obs_test` result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObsTestResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obs_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `obs_set_password` result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObsPasswordResult {
    pub stored: bool,
}

/// `shortcuts_import_preview` / `shortcuts_import_apply` argument.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportSource {
    Text { text: String },
    File { path: String },
}

/// `sessions_list` entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    /// ISO 8601 start time.
    pub started: String,
    pub duration_ms: u64,
    pub callouts: u32,
    pub markers: u32,
}

/// `session_export` format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Srt,
    Md,
    Json,
    Chapters,
}

/// `session_export` result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportResult {
    pub path: String,
}

/// One route in `server_urls`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteUrl {
    pub route: Route,
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// `server_urls` result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerUrls {
    pub port: u16,
    pub running: bool,
    pub error: Option<String>,
    pub routes: Vec<RouteUrl>,
}

/// `open_external_folder` argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderKind {
    Sessions,
    Config,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        let st = Status {
            paused: false,
            suppressed: Some(Suppressed::Password),
            profile: None,
            solidworks_mode: SolidworksMode::Auto,
            keyboard_visible: true,
            server: ServerStatus {
                port: 47820,
                running: true,
                error: None,
                clients: 1,
            },
            obs: ObsStatus {
                enabled: true,
                state: ObsState::AuthFailed,
                error: Some("Authentication failed".into()),
                recording: None,
            },
            session: None,
            hook: HookStatus {
                installed: true,
                dropped: 0,
            },
            version: "0.1.0".into(),
        };
        let v = serde_json::to_value(&st).unwrap();
        assert_eq!(v["suppressed"], "password");
        assert_eq!(v["obs"]["state"], "auth_failed");
        assert_eq!(v["hook"]["dropped"], 0);
        let back: Status = serde_json::from_value(v).unwrap();
        assert_eq!(back, st);

        let src: ImportSource = serde_json::from_str(r#"{"kind":"text","text":"a"}"#).unwrap();
        assert_eq!(src, ImportSource::Text { text: "a".into() });
        let src: ImportSource =
            serde_json::from_str(r#"{"kind":"file","path":"C:\\x.sldreg"}"#).unwrap();
        assert_eq!(
            src,
            ImportSource::File {
                path: "C:\\x.sldreg".into()
            }
        );
        assert_eq!(
            serde_json::to_string(&ExportFormat::Chapters).unwrap(),
            "\"chapters\""
        );
        assert_eq!(
            serde_json::to_string(&FolderKind::Sessions).unwrap(),
            "\"sessions\""
        );
        let ok = ObsTestResult {
            ok: true,
            obs_version: Some("32.0.0".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&ok).unwrap(),
            r#"{"ok":true,"obs_version":"32.0.0"}"#
        );
        let r = RouteUrl {
            route: Route::Keys,
            url: "http://127.0.0.1:47820/overlay/keys".into(),
            width: 800,
            height: 200,
        };
        assert_eq!(serde_json::to_value(&r).unwrap()["route"], "keys");
    }
}
