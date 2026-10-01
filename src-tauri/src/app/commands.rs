//! The settings window's commands (ARCHITECTURE.md section 12, plus the
//! additions `src/settings/ipc.ts` documents: `overlay_hello`,
//! `hotkeys_capture`, `hotkeys_status`, `platform_info`). Names and argument
//! shapes match the IPC client exactly; Tauri maps the camelCase JS keys to
//! these snake_case parameters (every key is one word or snake_case).
//!
//! Every command is async and does its work on a blocking thread, so the
//! main thread never waits on files, the server, OBS or the hotkey plugin.
//! Errors are short sentences for the window; they never quote typed text,
//! and nothing here logs arguments (they can hold an OBS password or pasted
//! shortcut text).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use keystage_core::import::{self, ImportPreview, ImportResult};
use keystage_core::ipc::{
    ExportFormat, FolderKind, ImportSource, Monitor, ObsPasswordResult, ObsTestResult, RouteUrl,
    ServerUrls, SessionSummary,
};
use keystage_core::protocol::{Route, ServerMsg, SolidworksMode};
use keystage_core::{Profile, Settings};
use keystage_obs::ObsConfig;
use keystage_session::Session;
use serde::Serialize;
use tauri::State;

use super::hotkeys::HotkeyState;
use super::obs_bridge;
use super::sessions::Origin;
use super::settings_store::{self, display_name, io_words, read_limited};
use super::shell::Shell;
use super::sizes;
use super::state::StatusPayload;
use crate::platform::{self, PlatformError};

type Res<T> = Result<T, String>;
type ShellState<'a> = State<'a, Arc<Shell>>;

/// Every command name, as registered (and as `build.rs` lists them for the
/// permission manifest).
pub const COMMANDS: [&str; 31] = [
    "get_settings",
    "save_settings",
    "reset_settings",
    "get_status",
    "list_monitors",
    "set_paused",
    "set_solidworks_mode",
    "set_keyboard_visible",
    "obs_test",
    "obs_set_password",
    "shortcuts_import_preview",
    "shortcuts_import_apply",
    "shortcuts_import_clear",
    "profile_shortcuts",
    "session_start",
    "session_stop",
    "session_marker",
    "sessions_list",
    "session_get",
    "session_rename_marker",
    "session_export",
    "session_delete",
    "settings_export",
    "settings_import",
    "server_urls",
    "play_sound_preview",
    "open_external_folder",
    "overlay_hello",
    "hotkeys_capture",
    "hotkeys_status",
    "platform_info",
];

/// Shortcut files larger than this are not exports.
pub const IMPORT_MAX_BYTES: u64 = 16 * 1024 * 1024;

const BUSY: &str = "The app could not finish that. Try again.";

/// Runs `f` on a blocking thread.
async fn blocking<T, F>(shell: &ShellState<'_>, f: F) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<Shell>) -> Res<T> + Send + 'static,
{
    let s = shell.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(s))
        .await
        .map_err(|_| BUSY.to_string())?
}

/// `session_export` result: the contract's `{ path }` plus the export's
/// warnings (fewer than three chapters, times from the session clock).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExportOutcome {
    pub path: String,
    pub warnings: Vec<String>,
}

/// `platform_info` result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlatformInfo {
    pub os_build: Option<u32>,
    pub exclude_from_capture_supported: bool,
}

/// A path chosen in a file dialog: absolute and not empty.
pub fn user_path(path: &str) -> Res<PathBuf> {
    let t = path.trim();
    if t.is_empty() {
        return Err("Choose where to save the file.".into());
    }
    let p = PathBuf::from(t);
    if !p.is_absolute() {
        return Err("Choose a file with the file dialog.".into());
    }
    Ok(p)
}

/// Reads an import source.
pub fn read_import(source: &ImportSource) -> Res<ImportResult> {
    match source {
        ImportSource::Text { text } => Ok(import::import_text(text)),
        ImportSource::File { path } => {
            let p = Path::new(path.trim());
            if path.trim().is_empty() {
                return Err("Choose a file to import.".into());
            }
            let name = display_name(p);
            let bytes = read_limited(p, IMPORT_MAX_BYTES)
                .map_err(|e| format!("Could not read {name}: {}", io_words(&e)))?;
            Ok(import::import_bytes(&bytes, Some(&name)))
        }
    }
}

/// The profile the settings window shows: the SolidWorks data merged with
/// the user's import, the general profile, or one of the user's profiles.
pub fn profile_for(id: &str, settings: &Settings, builtin: &Profile) -> Res<Profile> {
    match id {
        keystage_core::profile::SOLIDWORKS_ID => Ok(settings.profiles.solidworks_profile(builtin)),
        keystage_core::profile::GENERAL_ID => Ok(Profile::general()),
        other => settings
            .profiles
            .user
            .iter()
            .find(|p| p.id == other)
            .cloned()
            .ok_or_else(|| "There is no profile with that id.".to_string()),
    }
}

/// A port from the window, checked.
pub fn port_arg(v: &serde_json::Value) -> Option<u16> {
    let n = v.as_f64()?;
    if n.fract() != 0.0 || !(1.0..=65535.0).contains(&n) {
        return None;
    }
    Some(n as u16)
}

/// The server URLs and recommended OBS source sizes.
pub fn server_urls_for(
    port: u16,
    running: bool,
    error: Option<String>,
    settings: &Settings,
    monitor: (u32, u32),
) -> ServerUrls {
    let routes = [
        Route::Keys,
        Route::Keyboard,
        Route::Mouse,
        Route::Cursor,
        Route::All,
    ]
    .into_iter()
    .map(|route| {
        let (width, height) = sizes::obs_source_size(route, &settings.overlay, monitor);
        RouteUrl {
            route,
            url: keystage_server::overlay_url(port, route),
            width,
            height,
        }
    })
    .collect();
    ServerUrls {
        port,
        running,
        error,
        routes,
    }
}

/// Overwrites a secret's bytes before it is dropped (best effort).
fn wipe(s: String) {
    let mut b = s.into_bytes();
    b.fill(0);
    drop(b);
}

/* --- Settings ---------------------------------------------------------------- */

#[tauri::command]
pub async fn get_settings(shell: ShellState<'_>) -> Res<Settings> {
    Ok((*shell.settings()).clone())
}

#[tauri::command]
pub async fn save_settings(shell: ShellState<'_>, settings: serde_json::Value) -> Res<Settings> {
    blocking(&shell, move |s| {
        let next = s.replace_settings(|old| {
            settings_store::merge_incoming(old, &settings).map(|(next, _)| next)
        })?;
        s.clear_settings_notice();
        Ok((*next).clone())
    })
    .await
}

#[tauri::command]
pub async fn reset_settings(shell: ShellState<'_>) -> Res<Settings> {
    blocking(&shell, |s| {
        let next = s.replace_settings(|_| {
            let mut d = Settings {
                first_run_done: true,
                ..Settings::default()
            };
            d.obs.password_dpapi = None;
            Ok(d)
        })?;
        s.clear_settings_notice();
        Ok((*next).clone())
    })
    .await
}

#[tauri::command]
pub async fn settings_export(shell: ShellState<'_>, path: String) -> Res<Settings> {
    blocking(&shell, move |s| {
        let p = user_path(&path)?;
        let current = s.settings();
        settings_store::export_settings(&p, &current)?;
        Ok((*current).clone())
    })
    .await
}

#[tauri::command]
pub async fn settings_import(shell: ShellState<'_>, path: String) -> Res<Settings> {
    blocking(&shell, move |s| {
        let p = user_path(&path)?;
        let next = s.replace_settings(|old| {
            settings_store::import_settings(&p, old).map(|(next, _)| next)
        })?;
        s.clear_settings_notice();
        Ok((*next).clone())
    })
    .await
}

/* --- Status and live switches -------------------------------------------------- */

#[tauri::command]
pub async fn get_status(shell: ShellState<'_>) -> Res<StatusPayload> {
    blocking(&shell, |s| Ok(s.status())).await
}

#[tauri::command]
pub async fn list_monitors(shell: ShellState<'_>) -> Res<Vec<Monitor>> {
    blocking(&shell, |_| {
        platform::monitors().map_err(|_| "The monitors could not be read.".to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_paused(shell: ShellState<'_>, paused: bool) -> Res<StatusPayload> {
    blocking(&shell, move |s| {
        s.set_paused(paused);
        Ok(s.status())
    })
    .await
}

#[tauri::command]
pub async fn set_solidworks_mode(
    shell: ShellState<'_>,
    mode: SolidworksMode,
) -> Res<StatusPayload> {
    blocking(&shell, move |s| {
        s.set_mode(mode)?;
        Ok(s.status())
    })
    .await
}

#[tauri::command]
pub async fn set_keyboard_visible(shell: ShellState<'_>, visible: bool) -> Res<StatusPayload> {
    blocking(&shell, move |s| {
        s.set_keyboard_visible(visible);
        Ok(s.status())
    })
    .await
}

/* --- OBS ------------------------------------------------------------------------ */

#[tauri::command]
pub async fn obs_test(
    shell: ShellState<'_>,
    host: String,
    port: serde_json::Value,
    password: Option<String>,
) -> Res<ObsTestResult> {
    let Some(port) = port_arg(&port) else {
        return Ok(ObsTestResult {
            ok: false,
            error: Some("Type a port from 1 to 65535.".into()),
            ..ObsTestResult::default()
        });
    };
    let password = match password {
        Some(p) => Some(p),
        None => {
            let settings = shell.settings();
            let (stored, _) = obs_bridge::client_config(&settings.obs);
            stored.password.map(|p| p.expose().to_string())
        }
    };
    let cfg = ObsConfig::new(host.trim(), port, password);
    if let Err(e) = cfg.url() {
        return Ok(ObsTestResult {
            ok: false,
            error: Some(e),
            ..ObsTestResult::default()
        });
    }
    Ok(keystage_obs::test_connection(&cfg, obs_bridge::TEST_LIMIT).await)
}

#[tauri::command]
pub async fn obs_set_password(
    shell: ShellState<'_>,
    password: Option<String>,
) -> Res<ObsPasswordResult> {
    blocking(&shell, move |s| {
        let blob = match password {
            Some(p) if !p.is_empty() => {
                let r = platform::dpapi_protect(&p);
                wipe(p);
                Some(r.map_err(|e| {
                    match e {
                        PlatformError::Unsupported => {
                            "This computer cannot store the password securely, so it was not saved."
                                .to_string()
                        }
                        _ => "Windows could not protect the password, so it was not saved."
                            .to_string(),
                    }
                })?)
            }
            Some(p) => {
                wipe(p);
                None
            }
            None => None,
        };
        let stored = blob.is_some();
        s.update_settings(|st| st.obs.password_dpapi = blob)?;
        Ok(ObsPasswordResult { stored })
    })
    .await
}

/* --- SolidWorks shortcuts ---------------------------------------------------------- */

#[tauri::command]
pub async fn shortcuts_import_preview(
    shell: ShellState<'_>,
    source: ImportSource,
) -> Res<ImportPreview> {
    blocking(&shell, move |s| {
        let result = read_import(&source)?;
        Ok(import::preview(&result, &s.builtin))
    })
    .await
}

#[tauri::command]
pub async fn shortcuts_import_apply(shell: ShellState<'_>, source: ImportSource) -> Res<Settings> {
    blocking(&shell, move |s| {
        let result = read_import(&source)?;
        if result.entries.is_empty() {
            return Err("There is nothing to apply in that import.".into());
        }
        // A Copy List is the full list: it replaces the previous import.
        let next =
            s.update_settings(|st| st.profiles.solidworks_user_shortcuts = result.entries)?;
        Ok((*next).clone())
    })
    .await
}

#[tauri::command]
pub async fn shortcuts_import_clear(shell: ShellState<'_>) -> Res<Settings> {
    blocking(&shell, |s| {
        let next = s.update_settings(|st| st.profiles.solidworks_user_shortcuts.clear())?;
        Ok((*next).clone())
    })
    .await
}

#[tauri::command]
pub async fn profile_shortcuts(shell: ShellState<'_>, id: String) -> Res<Profile> {
    let settings = shell.settings();
    profile_for(&id, &settings, &shell.builtin)
}

/* --- Sessions ------------------------------------------------------------------------ */

#[tauri::command]
pub async fn session_start(shell: ShellState<'_>) -> Res<StatusPayload> {
    blocking(&shell, |s| {
        s.session_start(Origin::User);
        Ok(s.status())
    })
    .await
}

#[tauri::command]
pub async fn session_stop(shell: ShellState<'_>) -> Res<StatusPayload> {
    blocking(&shell, |s| {
        s.session_stop()?;
        Ok(s.status())
    })
    .await
}

#[tauri::command]
pub async fn session_marker(shell: ShellState<'_>, name: Option<String>) -> Res<StatusPayload> {
    blocking(&shell, move |s| {
        s.add_marker(name.as_deref().map(str::trim).filter(|n| !n.is_empty()))?;
        Ok(s.status())
    })
    .await
}

#[tauri::command]
pub async fn sessions_list(shell: ShellState<'_>) -> Res<Vec<SessionSummary>> {
    blocking(&shell, |s| Ok(s.sessions.list(platform::now_ms()))).await
}

#[tauri::command]
pub async fn session_get(shell: ShellState<'_>, id: String) -> Res<Session> {
    blocking(&shell, move |s| s.sessions.get(&id)).await
}

#[tauri::command]
pub async fn session_rename_marker(
    shell: ShellState<'_>,
    id: String,
    index: usize,
    name: String,
) -> Res<Session> {
    blocking(&shell, move |s| {
        let r = s.sessions.rename_marker(&id, index, &name);
        s.publish_state();
        r
    })
    .await
}

#[tauri::command]
pub async fn session_export(
    shell: ShellState<'_>,
    id: String,
    format: ExportFormat,
    path: String,
) -> Res<ExportOutcome> {
    blocking(&shell, move |s| {
        let p = user_path(&path)?;
        let linger = s.settings().overlay.callouts.linger_ms;
        let warnings = s.sessions.export(&id, format, &p, linger)?;
        Ok(ExportOutcome {
            path: p.display().to_string(),
            warnings,
        })
    })
    .await
}

#[tauri::command]
pub async fn session_delete(shell: ShellState<'_>, id: String) -> Res<Vec<SessionSummary>> {
    blocking(&shell, move |s| {
        s.sessions.delete(&id)?;
        Ok(s.sessions.list(platform::now_ms()))
    })
    .await
}

/* --- Server, sound, folders ----------------------------------------------------------- */

#[tauri::command]
pub async fn server_urls(shell: ShellState<'_>) -> Res<ServerUrls> {
    blocking(&shell, |s| {
        let settings = s.settings();
        let st = s.server.status();
        let monitors = s.monitors();
        let monitor =
            super::overlay_window::choose_monitor(&monitors, settings.host.monitor.as_deref())
                .map(|m| (m.width, m.height))
                .unwrap_or((1920, 1080));
        Ok(server_urls_for(
            st.port, st.running, st.error, &settings, monitor,
        ))
    })
    .await
}

#[tauri::command]
pub async fn play_sound_preview(shell: ShellState<'_>, pack: String) -> Res<()> {
    match &shell.sound {
        Some(s) => s.preview(&pack),
        None => Err("Sound playback is not available.".into()),
    }
}

#[tauri::command]
pub async fn open_external_folder(shell: ShellState<'_>, which: FolderKind) -> Res<()> {
    blocking(&shell, move |s| {
        use tauri_plugin_opener::OpenerExt;
        let folder = match which {
            FolderKind::Sessions => s.sessions.folder(),
            FolderKind::Config => s.paths.config_dir.clone(),
        };
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not open the folder: {}.", io_words(&e)))?;
        s.app
            .opener()
            .open_path(folder.display().to_string(), None::<&str>)
            .map_err(|_| "Windows could not open the folder.".to_string())
    })
    .await
}

/* --- Additions ---------------------------------------------------------------------------- */

#[tauri::command]
pub async fn overlay_hello(shell: ShellState<'_>) -> Res<ServerMsg> {
    Ok(shell.hello())
}

#[tauri::command]
pub async fn hotkeys_capture(shell: ShellState<'_>, active: bool) -> Res<()> {
    blocking(&shell, move |s| {
        let settings = s.settings();
        s.hotkeys.capture(&s.app, active, &settings.hotkeys);
        s.refresh_soon();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn hotkeys_status(shell: ShellState<'_>) -> Res<Vec<HotkeyState>> {
    Ok(shell.hotkeys.states())
}

#[tauri::command]
pub async fn platform_info() -> Res<PlatformInfo> {
    Ok(PlatformInfo {
        os_build: platform::os_build(),
        exclude_from_capture_supported: platform::capture_exclusion_supported(),
    })
}

/// The invoke handler with every command.
pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        get_settings,
        save_settings,
        reset_settings,
        get_status,
        list_monitors,
        set_paused,
        set_solidworks_mode,
        set_keyboard_visible,
        obs_test,
        obs_set_password,
        shortcuts_import_preview,
        shortcuts_import_apply,
        shortcuts_import_clear,
        profile_shortcuts,
        session_start,
        session_stop,
        session_marker,
        sessions_list,
        session_get,
        session_rename_marker,
        session_export,
        session_delete,
        settings_export,
        settings_import,
        server_urls,
        play_sound_preview,
        open_external_folder,
        overlay_hello,
        hotkeys_capture,
        hotkeys_status,
        platform_info,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPC_TS: &str = include_str!("../../../src/settings/ipc.ts");
    const BUILD_RS: &str = include_str!("../../build.rs");
    const SETTINGS_CAP: &str = include_str!("../../capabilities/settings.json");
    const OVERLAY_CAP: &str = include_str!("../../capabilities/overlay.json");
    const SOURCE: &str = include_str!("commands.rs");

    #[test]
    fn every_ipc_command_is_implemented_and_permitted() {
        for name in COMMANDS {
            assert!(
                IPC_TS.contains(&format!("'{name}'")),
                "{name} is not in src/settings/ipc.ts"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{name}\"")),
                "{name} is not in build.rs"
            );
            assert!(
                SOURCE.contains(&format!("pub async fn {name}(")),
                "{name} has no command function"
            );
            let perm = format!("\"allow-{}\"", name.replace('_', "-"));
            assert!(
                SETTINGS_CAP.contains(&perm),
                "{perm} missing for the settings window"
            );
        }
        // Every command the IPC client names is in the list.
        let mut client_names = 0;
        let block = IPC_TS
            .split("export const COMMANDS = {")
            .nth(1)
            .and_then(|rest| rest.split("} as const;").next())
            .expect("the COMMANDS block");
        for line in block.lines() {
            let t = line.trim();
            if let Some((_, rest)) = t.split_once(": '") {
                if let Some(name) = rest.strip_suffix("',") {
                    if name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                        client_names += 1;
                        assert!(COMMANDS.contains(&name), "{name} is not implemented");
                    }
                }
            }
        }
        assert_eq!(client_names, COMMANDS.len());
    }

    #[test]
    fn the_overlay_window_gets_only_what_it_needs() {
        let v: serde_json::Value = serde_json::from_str(OVERLAY_CAP).expect("json");
        let perms: Vec<String> = v["permissions"]
            .as_array()
            .expect("array")
            .iter()
            .map(|p| p.as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(
            perms,
            vec![
                "core:event:allow-listen",
                "core:event:allow-unlisten",
                "allow-overlay-hello"
            ]
        );
    }

    #[test]
    fn paths_from_dialogs() {
        assert!(user_path("").is_err());
        assert!(user_path("relative.srt").is_err());
        #[cfg(windows)]
        let abs = "C:\\Users\\x\\video.srt";
        #[cfg(not(windows))]
        let abs = "/home/x/video.srt";
        assert_eq!(user_path(abs).expect("ok"), PathBuf::from(abs));
    }

    #[test]
    fn ports_from_the_window() {
        assert_eq!(port_arg(&serde_json::json!(4455)), Some(4455));
        assert_eq!(port_arg(&serde_json::json!(4455.0)), Some(4455));
        assert_eq!(port_arg(&serde_json::json!(0)), None);
        assert_eq!(port_arg(&serde_json::json!(70000)), None);
        assert_eq!(port_arg(&serde_json::json!(44.5)), None);
        assert_eq!(port_arg(&serde_json::json!("4455")), None);
    }

    #[test]
    fn imports_from_text_and_files() {
        let r = read_import(&ImportSource::Text {
            text: "Category\tCommand\tShortcut(s)\nFixture\tFixture Command One\tCtrl+B\n".into(),
        })
        .expect("text");
        assert_eq!(r.entries.len(), 1);
        let d = tempfile::tempdir().expect("d");
        let p = d.path().join("list.txt");
        std::fs::write(
            &p,
            "Category,Command,Shortcut(s)\r\nFixture,Fixture Command Two,Ctrl+Shift+Q\r\n",
        )
        .expect("w");
        let r = read_import(&ImportSource::File {
            path: p.display().to_string(),
        })
        .expect("file");
        assert_eq!(r.entries.len(), 1);
        let missing = read_import(&ImportSource::File {
            path: d.path().join("nope.sldreg").display().to_string(),
        })
        .expect_err("missing");
        assert!(
            missing.starts_with("Could not read nope.sldreg"),
            "{missing}"
        );
        assert!(read_import(&ImportSource::File { path: " ".into() }).is_err());
    }

    #[test]
    fn profiles_for_the_window() {
        let builtin = keystage_core::builtin::solidworks().expect("profile");
        let mut s = Settings::default();
        s.profiles
            .solidworks_user_shortcuts
            .push(keystage_core::UserShortcut {
                keys: vec!["Ctrl".into(), "B".into()],
                doc_keys: "Ctrl+B".into(),
                command: "Fixture Command One".into(),
                ..keystage_core::UserShortcut::default()
            });
        let p = profile_for("solidworks", &s, &builtin).expect("sw");
        assert_eq!(p.shortcuts[0].command, "Fixture Command One");
        assert_eq!(p.shortcuts.len(), builtin.shortcuts.len() + 1);
        assert_eq!(
            profile_for("general", &s, &builtin).expect("g").id,
            "general"
        );
        assert!(profile_for("mine", &s, &builtin).is_err());
    }

    #[test]
    fn urls_cover_every_route() {
        let s = Settings::default();
        let u = server_urls_for(47820, true, None, &s, (2560, 1440));
        assert_eq!(u.routes.len(), 5);
        assert_eq!(u.routes[0].url, "http://127.0.0.1:47820/overlay/keys");
        let all = u
            .routes
            .iter()
            .find(|r| r.route == Route::All)
            .expect("all");
        assert_eq!((all.width, all.height), (2560, 1440));
        let v = serde_json::to_value(&u).expect("v");
        assert_eq!(v["routes"][1]["route"], "keyboard");
    }

    #[test]
    fn outcome_shapes() {
        let v = serde_json::to_value(ExportOutcome {
            path: "x".into(),
            warnings: vec![],
        })
        .expect("v");
        assert_eq!(v["path"], "x");
        let v = serde_json::to_value(PlatformInfo {
            os_build: None,
            exclude_from_capture_supported: false,
        })
        .expect("v");
        assert_eq!(v["os_build"], serde_json::Value::Null);
    }
}
