//! Host A, the native overlay window (ARCHITECTURE.md section 6, Host A;
//! `.work/research/stack.md` A.3 and A.6).
//!
//! * Built hidden with `transparent`, no decorations, no shadow, always on
//!   top, no taskbar button, not focused, not focusable (tao adds
//!   `WS_EX_NOACTIVATE`), not resizable, `content_protected(false)`.
//! * Still hidden: the platform style guard is installed on the main thread
//!   (Alt+Tab and taskbar exclusion that survives tao's style rewrites),
//!   `set_ignore_cursor_events(true)` makes it click-through, and it is
//!   moved and sized to the chosen monitor's rectangle in physical pixels.
//! * Then shown once and put on top. tao clears its "do not focus" marker
//!   right after creation, so this first show is tao's `SW_SHOW` on a window
//!   that already carries `WS_EX_NOACTIVATE`; whether Windows keeps the
//!   focus on SolidWorks must be confirmed on Windows (stack.md F.2).
//! * Never hidden at the OS level afterwards: the renderer draws nothing
//!   when `AppState.native_visible` is false. Turning the overlay off, or a
//!   new monitor rectangle, closes the window and builds a new one.
//! * Capture exclusion goes only through `platform::set_capture_excluded`,
//!   which refuses below Windows build 19041.
//!
//! Every function that touches the window runs on the main thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use keystage_core::ipc::Monitor;
use keystage_core::{MonitorRect, Settings};
use parking_lot::Mutex;
use tauri::{Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::platform;

/// Labels are `overlay-1`, `overlay-2`, ...: a new window never waits for
/// the old label to be released.
pub const LABEL_PREFIX: &str = "overlay-";
/// The bundled page, as the native host on the all-in-one route.
pub const URL: &str = "overlay.html?host=native&route=all";

/// Where Host A goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// GDI device name of the monitor.
    pub monitor: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Placement {
    pub fn rect(&self) -> MonitorRect {
        MonitorRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// The chosen monitor: the one named in settings, else the primary, else the
/// first one listed.
pub fn choose_monitor<'a>(monitors: &'a [Monitor], wanted: Option<&str>) -> Option<&'a Monitor> {
    wanted
        .and_then(|w| monitors.iter().find(|m| m.id.eq_ignore_ascii_case(w)))
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or_else(|| monitors.first())
}

/// The chosen monitor's rectangle, for mouse normalization and Host A.
pub fn monitor_rect(monitors: &[Monitor], settings: &Settings) -> Option<MonitorRect> {
    choose_monitor(monitors, settings.host.monitor.as_deref()).map(|m| MonitorRect {
        x: m.x,
        y: m.y,
        width: m.width,
        height: m.height,
    })
}

/// Where Host A should be, or `None` when it should not exist (turned off,
/// or no monitor is known).
pub fn placement(monitors: &[Monitor], settings: &Settings) -> Option<Placement> {
    if !settings.host.native_overlay {
        return None;
    }
    let m = choose_monitor(monitors, settings.host.monitor.as_deref())?;
    if m.width == 0 || m.height == 0 {
        return None;
    }
    Some(Placement {
        monitor: m.id.clone(),
        x: m.x,
        y: m.y,
        width: m.width,
        height: m.height,
    })
}

/// What reconciling has to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    Close,
    Create(Placement),
    Recreate(Placement),
}

/// Decides between keeping, closing, creating or recreating the window.
pub fn plan(current: Option<&Placement>, desired: Option<&Placement>) -> Step {
    match (current, desired) {
        (None, None) => Step::Nothing,
        (Some(_), None) => Step::Close,
        (None, Some(d)) => Step::Create(d.clone()),
        (Some(c), Some(d)) if c == d => Step::Nothing,
        (Some(_), Some(d)) => Step::Recreate(d.clone()),
    }
}

/// Owns Host A.
#[derive(Debug, Default)]
pub struct OverlayManager {
    current: Mutex<Option<(String, Placement)>>,
    excluded: Mutex<Option<bool>>,
    open: AtomicBool,
    next_id: AtomicU64,
}

impl OverlayManager {
    pub fn new() -> OverlayManager {
        OverlayManager::default()
    }

    /// True while Host A exists (the fan-out skips Tauri events otherwise).
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// The current window's label.
    pub fn label(&self) -> Option<String> {
        self.current.lock().as_ref().map(|(l, _)| l.clone())
    }

    /// Brings Host A in line with `desired`. Main thread only.
    pub fn reconcile<R: Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        desired: Option<Placement>,
        exclude_from_capture: bool,
    ) {
        let current = self.current.lock().as_ref().map(|(_, p)| p.clone());
        match plan(current.as_ref(), desired.as_ref()) {
            Step::Nothing => {}
            Step::Close => self.close(app),
            Step::Create(p) => self.create(app, p),
            Step::Recreate(p) => {
                self.close(app);
                self.create(app, p);
            }
        }
        self.apply_exclusion(app, exclude_from_capture);
    }

    fn close<R: Runtime>(&self, app: &tauri::AppHandle<R>) {
        let old = self.current.lock().take();
        self.open.store(false, Ordering::Release);
        *self.excluded.lock() = None;
        if let Some((label, _)) = old {
            if let Some(w) = app.get_webview_window(&label) {
                if let Err(e) = w.destroy() {
                    tracing::warn!(error = %e, "overlay window did not close");
                }
            }
            tracing::info!("overlay window closed");
        }
    }

    fn create<R: Runtime>(&self, app: &tauri::AppHandle<R>, p: Placement) {
        let n = self.next_id.fetch_add(1, Ordering::AcqRel) + 1;
        let label = format!("{LABEL_PREFIX}{n}");
        match build(app, &label, &p) {
            Ok(()) => {
                *self.current.lock() = Some((label, p));
                self.open.store(true, Ordering::Release);
                tracing::info!("overlay window created");
            }
            Err(e) => {
                tracing::error!(error = %e, "overlay window could not be created");
                if let Some(w) = app.get_webview_window(&label) {
                    let _ = w.destroy();
                }
            }
        }
    }

    fn apply_exclusion<R: Runtime>(&self, app: &tauri::AppHandle<R>, want: bool) {
        let Some(label) = self.label() else {
            return;
        };
        if *self.excluded.lock() == Some(want) {
            return;
        }
        if let Some(w) = app.get_webview_window(&label) {
            match platform::set_capture_excluded(&w, want) {
                Ok(_) => *self.excluded.lock() = Some(want),
                Err(e) => tracing::warn!(error = %e, "capture exclusion not applied"),
            }
        }
    }

    /// Puts Host A back on top of other topmost windows. Main thread only.
    pub fn reassert_topmost<R: Runtime>(&self, app: &tauri::AppHandle<R>) {
        let Some(label) = self.label() else {
            return;
        };
        if let Some(w) = app.get_webview_window(&label) {
            if let Err(e) = platform::reassert_topmost(&w) {
                tracing::warn!(error = %e, "overlay topmost not reasserted");
            }
        }
    }
}

fn build<R: Runtime>(app: &tauri::AppHandle<R>, label: &str, p: &Placement) -> Result<(), String> {
    let w = WebviewWindowBuilder::new(app, label, WebviewUrl::App(URL.into()))
        .title("IDEA Keystage overlay")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .resizable(false)
        .content_protected(false)
        .visible(false)
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| e.to_string())?;
    platform::install_overlay_guard(&w).map_err(|e| e.to_string())?;
    w.set_ignore_cursor_events(true)
        .map_err(|e| e.to_string())?;
    place(&w, p)?;
    w.show().map_err(|e| e.to_string())?;
    // Moving onto a monitor with another DPI can make Windows resize the
    // window to keep its logical size; put the physical rectangle back.
    place(&w, p)?;
    platform::reassert_topmost(&w).map_err(|e| e.to_string())?;
    Ok(())
}

fn place<R: Runtime>(w: &tauri::WebviewWindow<R>, p: &Placement) -> Result<(), String> {
    w.set_position(PhysicalPosition::new(p.x, p.y))
        .map_err(|e| e.to_string())?;
    w.set_size(PhysicalSize::new(p.width, p.height))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(id: &str, x: i32, w: u32, primary: bool) -> Monitor {
        Monitor {
            id: id.into(),
            name: id.into(),
            x,
            y: 0,
            width: w,
            height: 1080,
            scale: 1.0,
            primary,
        }
    }

    #[test]
    fn the_chosen_monitor_falls_back_to_the_primary() {
        let ms = vec![
            mon("\\\\.\\DISPLAY2", -2560, 2560, false),
            mon("\\\\.\\DISPLAY1", 0, 1920, true),
        ];
        assert_eq!(choose_monitor(&ms, None).map(|m| m.x), Some(0));
        assert_eq!(
            choose_monitor(&ms, Some("\\\\.\\display2")).map(|m| m.x),
            Some(-2560)
        );
        assert_eq!(
            choose_monitor(&ms, Some("\\\\.\\DISPLAY9")).map(|m| m.x),
            Some(0)
        );
        let only = vec![mon("\\\\.\\DISPLAY3", 100, 800, false)];
        assert_eq!(choose_monitor(&only, None).map(|m| m.x), Some(100));
        assert!(choose_monitor(&[], None).is_none());
    }

    #[test]
    fn placement_follows_the_settings() {
        let ms = vec![mon("\\\\.\\DISPLAY1", 0, 1920, true)];
        let mut s = Settings::default();
        let p = placement(&ms, &s).expect("placed");
        assert_eq!((p.x, p.y, p.width, p.height), (0, 0, 1920, 1080));
        assert_eq!(p.rect().width, 1920);
        assert_eq!(monitor_rect(&ms, &s).map(|r| r.height), Some(1080));
        s.host.native_overlay = false;
        assert!(placement(&ms, &s).is_none());
        assert!(monitor_rect(&ms, &s).is_some(), "mouse normalization stays");
        s.host.native_overlay = true;
        assert!(placement(&[], &s).is_none());
    }

    #[test]
    fn reconcile_plan() {
        let a = Placement {
            monitor: "A".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let b = Placement {
            width: 2560,
            ..a.clone()
        };
        assert_eq!(plan(None, None), Step::Nothing);
        assert_eq!(plan(Some(&a), None), Step::Close);
        assert_eq!(plan(None, Some(&a)), Step::Create(a.clone()));
        assert_eq!(plan(Some(&a), Some(&a)), Step::Nothing);
        assert_eq!(plan(Some(&a), Some(&b)), Step::Recreate(b.clone()));
    }

    #[test]
    fn the_page_is_the_native_host() {
        assert!(URL.starts_with("overlay.html?"));
        assert!(URL.contains("host=native"));
        assert!(URL.contains("route=all"));
    }
}
