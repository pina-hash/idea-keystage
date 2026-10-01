//! The settings window (label `settings`, `settings.html`). Opened on the
//! first run (the page shows the first-run wizard while
//! `settings.first_run_done` is false), at start when
//! `settings.startup.show_settings` is on, from the tray, and when the app is
//! launched a second time. Closing it destroys it; the app keeps running in
//! the tray.
//!
//! The live preview in this window receives `keystage://overlay` messages
//! only while the window is open and not minimized.

use tauri::{Manager, Runtime, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "settings";
pub const URL: &str = "settings.html";
pub const TITLE: &str = "IDEA Keystage settings";
/// Default and minimum inner size in logical pixels.
pub const SIZE: (f64, f64) = (1200.0, 800.0);
pub const MIN_SIZE: (f64, f64) = (980.0, 680.0);
/// The page's first-frame background (`settings.html`), so the window never
/// flashes white before the page paints.
pub const BACKGROUND: (u8, u8, u8) = (0x12, 0x14, 0x13);

/// Opens the settings window, or brings it forward. Main thread only.
/// `on_visibility` is told whether the window can be seen (open and not
/// minimized), from the window's own events.
pub fn open<R, F>(app: &tauri::AppHandle<R>, on_visibility: F) -> Result<(), String>
where
    R: Runtime,
    F: Fn(bool) + Send + Sync + 'static,
{
    if let Some(w) = app.get_webview_window(LABEL) {
        if w.is_minimized().unwrap_or(false) {
            let _ = w.unminimize();
        }
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
        on_visibility(true);
        return Ok(());
    }
    let (r, g, b) = BACKGROUND;
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(URL.into()))
        .title(TITLE)
        .inner_size(SIZE.0, SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .center()
        .decorations(true)
        .resizable(true)
        .focused(true)
        .visible(true)
        .background_color(tauri::window::Color(r, g, b, 255))
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| e.to_string())?;
    on_visibility(true);
    let handle = w.clone();
    w.on_window_event(move |ev| match ev {
        WindowEvent::Destroyed => on_visibility(false),
        WindowEvent::Resized(_) | WindowEvent::Focused(_) | WindowEvent::Moved(_) => {
            let visible =
                handle.is_visible().unwrap_or(false) && !handle.is_minimized().unwrap_or(false);
            on_visibility(visible);
        }
        _ => {}
    });
    tracing::info!("settings window opened");
    Ok(())
}

/// Whether to open the settings window at start.
pub fn open_at_start(first_run_done: bool, show_settings: bool, autostarted: bool) -> bool {
    if !first_run_done {
        return true;
    }
    !autostarted && show_settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_rules() {
        assert!(
            open_at_start(false, false, true),
            "the first run always opens"
        );
        assert!(open_at_start(true, true, false));
        assert!(
            !open_at_start(true, true, true),
            "autostart stays in the tray"
        );
        assert!(!open_at_start(true, false, false));
    }

    #[test]
    fn sizes() {
        assert!(SIZE.0 >= MIN_SIZE.0 && SIZE.1 >= MIN_SIZE.1);
        assert_eq!(MIN_SIZE, (980.0, 680.0));
    }
}
