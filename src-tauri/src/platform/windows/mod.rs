//! The Windows implementation of the platform layer.
//!
//! Threads it creates:
//!
//! * `keystage-hooks`: installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` and pumps
//!   their messages ([`hooks`]). Owns no windows, makes no COM, UI
//!   Automation, process or file calls.
//! * `keystage-hook-watchdog`: reinstalls hooks Windows removed silently.
//! * `keystage-watcher`: a message-only window with WinEvent hooks for
//!   foreground and focus changes, session notifications and a poll timer
//!   ([`watcher`]).
//! * `keystage-uia`: a COM MTA thread with no windows that owns the single
//!   UI Automation focus-changed handler ([`uia`]).
//!
//! The functions at this level are what `platform/mod.rs` dispatches to.

pub mod dpapi;
pub mod hooks;
pub mod layout;
pub mod monitors;
pub mod os;
pub mod process;
#[cfg(test)]
mod tests;
mod uia;
pub mod watcher;
pub mod window_style;

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;

use keystage_core::ipc::Monitor;
use windows::Win32::Foundation::HWND;

pub use hooks::{start_input, InputHandle};
pub use watcher::{start_watcher, WatcherHandle};

use super::{secret, ForegroundInfo, LegendTable, ModifierSnapshot, PlatformError};

/// Set by the watcher while the foreground is elevated or unidentified: the
/// hooks most likely see no input then, so the watchdog stays quiet and a
/// `GetAsyncKeyState` read is not trusted.
pub(crate) static FOREGROUND_BLIND: AtomicBool = AtomicBool::new(false);

/// Set by the watcher between `WTS_SESSION_LOCK` and `WTS_SESSION_UNLOCK`.
pub(crate) static SESSION_LOCKED: AtomicBool = AtomicBool::new(false);

/// The foreground keyboard layout (HKL bits) as last seen by the watcher;
/// 0 until known. Used to fill scan codes of injected keys.
pub(crate) static CURRENT_HKL: AtomicIsize = AtomicIsize::new(0);

/// True when a `GetAsyncKeyState` read can be trusted right now.
pub(crate) fn desktop_readable() -> bool {
    !FOREGROUND_BLIND.load(Ordering::Acquire) && !SESSION_LOCKED.load(Ordering::Acquire)
}

pub fn now_ms() -> u64 {
    os::tick_count64()
}

pub fn monitors() -> Result<Vec<Monitor>, PlatformError> {
    monitors::list()
}

pub fn legends_snapshot() -> Arc<LegendTable> {
    Arc::new(layout::snapshot())
}

pub fn read_modifiers() -> Option<ModifierSnapshot> {
    if !desktop_readable() {
        return None;
    }
    Some(process::modifier_snapshot(now_ms()))
}

pub fn foreground_process() -> ForegroundInfo {
    process::foreground_now()
}

fn hwnd_of<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Result<HWND, PlatformError> {
    window
        .hwnd()
        .map_err(|_| PlatformError::InvalidInput("the window has no native handle"))
}

pub fn install_overlay_guard<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<(), PlatformError> {
    window_style::install_overlay_guard(hwnd_of(window)?)
}

pub fn reassert_topmost<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<(), PlatformError> {
    window_style::reassert_topmost(hwnd_of(window)?)
}

pub fn set_capture_excluded<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    excluded: bool,
) -> Result<bool, PlatformError> {
    window_style::set_capture_excluded(hwnd_of(window)?, excluded)
}

pub fn capture_exclusion_supported() -> bool {
    super::display::capture_exclusion_supported_on(os::build())
}

pub fn window_scale<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Option<f64> {
    window_style::window_scale(hwnd_of(window).ok()?)
}

pub fn dpapi_protect(plain: &str) -> Result<String, PlatformError> {
    secret::protect_with(plain, dpapi::protect)
}

pub fn dpapi_unprotect(blob_b64: &str) -> Result<String, PlatformError> {
    secret::unprotect_with(blob_b64, dpapi::unprotect)
}

pub fn os_build() -> Option<u32> {
    os::build()
}
