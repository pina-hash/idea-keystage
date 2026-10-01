//! Platform layer: the only code that talks to the operating system about
//! input, focus, windows, monitors and secrets (ARCHITECTURE.md sections 1,
//! 6 and 8). The Windows implementation lives in `windows/`; `stub.rs` keeps
//! the app compiling elsewhere, where input capture is not available.
//!
//! The app shell calls the functions below. Each one documents the thread it
//! may run on. In short:
//!
//! * [`start_input`] spawns the hook thread (plus a watchdog); the returned
//!   [`InputReceiver`] belongs to the consumer thread you pass in.
//! * [`start_watcher`] spawns the watcher thread (plus a UI Automation
//!   thread); its [`ContextEvent`] receiver is read by the same consumer.
//! * [`install_overlay_guard`], [`reassert_topmost`] and
//!   [`set_capture_excluded`] touch the overlay window and must run on the
//!   main thread (inside `setup` or `run_on_main_thread`).
//! * Everything else may run on any thread, never inside a hook procedure.
//!
//! Privacy (section 8): nothing here logs key names, typed text, window
//! titles or process names. `tracing` messages are lifecycle only.

mod context;
pub mod convert;
pub mod display;
mod error;
mod input;
pub mod legend_table;
pub mod secret;

#[cfg(not(windows))]
mod stub;
#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
use self::stub as backend;
#[cfg(windows)]
use self::windows as backend;

use std::sync::Arc;
use std::thread::Thread;

use crossbeam_channel::Receiver;
use keystage_core::ipc::Monitor;

pub use backend::{InputHandle, WatcherHandle};
pub use context::{
    classify_foreground, exe_file_name, is_edit_class, win32_verdict, ContextEvent, ForegroundInfo,
    ForegroundKind, ModifierSnapshot, PasswordGate, Verdict,
};
pub use error::PlatformError;
pub use input::{InputReceiver, QUEUE_CAPACITY};
pub use legend_table::LegendTable;

/// The app's monotonic millisecond clock. Hook timestamps are rebased onto
/// it, so `Processor::tick(now_ms())` and event times agree. On Windows it
/// is `GetTickCount64` (milliseconds since boot, 10 to 16 ms resolution, the
/// same counter the hooks stamp events with).
///
/// Threading: any thread.
pub fn now_ms() -> u64 {
    backend::now_ms()
}

/// Installs the low-level keyboard and mouse hooks on a dedicated thread.
///
/// `consumer` is the pipeline thread: the hook procedures unpark it after
/// every record. It is stored for the life of the process; a later call
/// must pass the same thread. Only one input hook runs at a time
/// ([`PlatformError::AlreadyRunning`]).
///
/// Returns the control handle (pause, resume, status, shutdown; cheap to
/// clone and usable from any thread) and the receiver the consumer drains.
/// On platforms without hooks the handle reports `is_installed() == false`
/// and the receiver stays empty.
///
/// Threading: call from any thread except inside a hook. Blocks until the
/// hooks are installed (at most 5 seconds).
pub fn start_input(consumer: Thread) -> Result<(InputHandle, InputReceiver), PlatformError> {
    backend::start_input(consumer)
}

/// Starts the watcher: foreground process, keyboard layout, password
/// focus, session lock and unlock. Events arrive on the returned channel and
/// `consumer` is unparked after each one. The first events describe the
/// current state (`Foreground`, `Modifiers` when readable, `Layout`,
/// `Password`).
///
/// Threading: call from any thread. Blocks until the watcher's window and
/// hooks exist (at most 5 seconds). Only one watcher runs at a time.
pub fn start_watcher(
    consumer: Thread,
) -> Result<(WatcherHandle, Receiver<ContextEvent>), PlatformError> {
    backend::start_watcher(consumer)
}

/// The connected monitors in physical pixels, primary first, with distinct
/// readable names (`id` is the GDI device name, e.g. `\\.\DISPLAY1`, which
/// is what `Settings.host.monitor` stores).
///
/// Threading: any thread. The process is per-monitor DPI aware v2 from its
/// manifest, so the rectangles are physical pixels on every thread.
pub fn monitors() -> Result<Vec<Monitor>, PlatformError> {
    backend::monitors()
}

/// The legend table of the foreground window's keyboard layout, built now.
/// The watcher already sends `ContextEvent::Layout` on every change; use
/// this for a one-off read (for example before the watcher starts).
///
/// Threading: any thread except a hook procedure. Takes well under a
/// millisecond.
pub fn legends_snapshot() -> Arc<LegendTable> {
    backend::legends_snapshot()
}

/// The modifiers held right now (`GetAsyncKeyState`), or `None` when the
/// answer cannot be trusted: the foreground is elevated or unknown, or the
/// session is locked (a zero from `GetAsyncKeyState` can mean "the call
/// failed"). A modifier key whose event the input receiver handed out in the
/// last 100 ms keeps the state that event gave it, because the asynchronous
/// state is updated only after the hook returns. Call it when
/// `Processor::needs_resync(now)` is true, after draining the receiver, and
/// forward `to_input_event()`. (Resume and foreground changes need no call:
/// the receiver adds a `ModifierResync` itself.)
///
/// Threading: any thread except a hook procedure.
pub fn read_modifiers() -> Option<ModifierSnapshot> {
    backend::read_modifiers()
}

/// The foreground window's process, read now (no cache).
///
/// Threading: any thread except a hook procedure.
pub fn foreground_process() -> ForegroundInfo {
    backend::foreground_process()
}

/// Keeps the overlay out of Alt+Tab and the taskbar and click-through for
/// its whole life: subclasses the window so every `GWL_EXSTYLE` change keeps
/// `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED`
/// on and `WS_EX_APPWINDOW` off, and applies that style once now.
///
/// Threading: **main thread only** (the thread that created the window;
/// "You cannot use the subclassing helper functions to subclass a window
/// across threads", SetWindowSubclass). Call it before the window's first
/// show. Returns [`PlatformError::WrongThread`] from any other thread. A
/// no-op off Windows.
pub fn install_overlay_guard<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<(), PlatformError> {
    backend::install_overlay_guard(window)
}

/// Puts the overlay back on top of other topmost windows without activating
/// it (`SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE | SWP_NOMOVE |
/// SWP_NOSIZE)`). The watcher already does this for the window passed to
/// [`install_overlay_guard`] on every foreground window change (posted to
/// the window's thread, so it never waits), including changes between two
/// windows of one process; calling it on `ContextEvent::Foreground` as well
/// is harmless.
///
/// Threading: main thread (from another thread `SetWindowPos` would wait
/// for the main thread to process the call). A no-op off Windows.
pub fn reassert_topmost<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<(), PlatformError> {
    backend::reassert_topmost(window)
}

/// Hides the overlay from screen capture (`WDA_EXCLUDEFROMCAPTURE`) or shows
/// it again (`WDA_NONE`). Excluding is applied only on Windows 10 build
/// 19041 or later; on older builds Windows would record an empty rectangle,
/// so the window is left capturable. Returns whether the window is now
/// excluded.
///
/// Threading: main thread. Off Windows: `Ok(false)`.
pub fn set_capture_excluded<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    excluded: bool,
) -> Result<bool, PlatformError> {
    backend::set_capture_excluded(window, excluded)
}

/// True when this Windows build can exclude a window from capture (build
/// 19041 or later). Settings should disable the option otherwise.
///
/// Threading: any thread.
pub fn capture_exclusion_supported() -> bool {
    backend::capture_exclusion_supported()
}

/// The DPI scale of the monitor the window is on (`GetDpiForWindow` / 96),
/// or `None` if it cannot be read.
///
/// Threading: any thread.
pub fn window_scale<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Option<f64> {
    backend::window_scale(window)
}

/// Encrypts a secret for the current Windows user (DPAPI, CurrentUser scope,
/// with an application entropy value) and returns the blob as base64, the
/// form `Settings.obs.password_dpapi` stores. Off Windows this is
/// [`PlatformError::Unsupported`]: a secret is never stored in plain text.
///
/// Threading: any thread.
pub fn dpapi_protect(plain: &str) -> Result<String, PlatformError> {
    backend::dpapi_protect(plain)
}

/// Decrypts a blob made by [`dpapi_protect`] for the same Windows user.
/// Returns [`PlatformError::InvalidData`] for anything that does not decrypt
/// to a valid secret (another user's blob, tampering, garbage).
///
/// Threading: any thread.
pub fn dpapi_unprotect(blob_b64: &str) -> Result<String, PlatformError> {
    backend::dpapi_unprotect(blob_b64)
}

/// The Windows build number (17763 for 1809, 19041 for 2004), read with
/// `RtlGetVersion` because `GetVersionExW` reports 6.2 to apps without a
/// compatibility manifest. `None` off Windows.
///
/// Threading: any thread.
pub fn os_build() -> Option<u32> {
    backend::os_build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send_sync<T: Send + Sync>() {}
    fn send<T: Send>() {}

    #[test]
    fn handles_can_live_in_app_state_and_cross_threads() {
        // Tauri state needs Send + Sync; the receiver moves to the pipeline.
        send_sync::<InputHandle>();
        send_sync::<WatcherHandle>();
        send::<InputReceiver>();
        send::<ContextEvent>();
        send_sync::<LegendTable>();
        send_sync::<PlatformError>();
    }

    #[test]
    fn clock_never_goes_backwards() {
        let a = now_ms();
        let b = now_ms();
        assert!(b >= a);
    }
}
