//! App shell (ARCHITECTURE.md section 2, surface e): pipeline wiring, tray,
//! hotkeys, IPC commands, settings store, sound playback, the overlay and
//! settings windows. `shell.rs` ties them together; the other modules keep
//! their logic testable without a Tauri runtime.

pub mod actions;
pub mod commands;
pub mod hotkeys;
pub mod icons;
pub mod logging;
pub mod obs_bridge;
pub mod overlay_window;
pub mod pipeline;
pub mod server;
pub mod sessions;
pub mod settings_store;
pub mod settings_window;
pub mod shell;
pub mod sizes;
pub mod sound;
pub mod state;
pub mod tray;

use std::sync::Arc;

use tauri::{Manager, RunEvent, Runtime};
use tauri_plugin_global_shortcut::ShortcutState;

pub use shell::{setup, Shell, AUTOSTART_ARG};

/// The global shortcut plugin with the app's handler. The handler runs on
/// the main thread under the plugin's lock: it only queues the action.
pub fn hotkeys_plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            if let Some(shell) = app.try_state::<Arc<Shell>>() {
                if let Some(name) = shell.hotkeys.lookup(shortcut.id()) {
                    shell.dispatch(actions::Action::from_hotkey(name));
                }
            }
        })
        .build()
}

/// A second launch brings the settings window forward.
pub fn on_second_instance<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(shell) = app.try_state::<Arc<Shell>>() {
        shell.open_settings();
    }
}

/// App lifecycle: closing the last window keeps the app in the tray; quitting
/// stops every service and saves a running Session Log.
pub fn on_run_event(app: &tauri::AppHandle<tauri::Wry>, event: RunEvent) {
    match event {
        RunEvent::ExitRequested { code, api, .. } => {
            let quitting = app
                .try_state::<Arc<Shell>>()
                .is_some_and(|s| s.is_quitting());
            if code.is_none() && !quitting {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            if let Some(shell) = app.try_state::<Arc<Shell>>() {
                shell.shutdown();
            }
        }
        _ => {}
    }
}
