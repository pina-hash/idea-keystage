//! Build script: embeds the app manifest (Common Controls v6 and per-monitor
//! DPI awareness v2, docs/DECISIONS.md item 2) and generates one permission
//! per IPC command, so each window's capability grants only the commands it
//! uses (`capabilities/settings.json`, `capabilities/overlay.json`).

/// Every IPC command; must match `src/app/commands.rs` (a unit test checks).
const COMMANDS: &[&str] = &[
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

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    let attrs = tauri_build::Attributes::new()
        .windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        )
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attrs).expect("tauri-build failed");
}
