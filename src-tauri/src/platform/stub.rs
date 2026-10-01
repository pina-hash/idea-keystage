//! The platform layer where there are no Windows hooks (Linux and macOS
//! builds, used for development and tests). Everything compiles and behaves
//! consistently: no input is captured, the watcher reports the US layout
//! once, monitors are unknown, and secrets cannot be stored.

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::thread::Thread;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};
use keystage_core::ipc::Monitor;
use keystage_core::{Mod, Side};

use super::input::{InputReceiver, InputShared};
use super::{ContextEvent, ForegroundInfo, LegendTable, ModifierSnapshot, PlatformError};

/// Milliseconds since the first call in this process (monotonic).
pub fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Control handle for the (absent) input hooks.
#[derive(Clone, Debug)]
pub struct InputHandle {
    shared: Arc<InputShared>,
}

impl InputHandle {
    /// Marks input as paused (nothing is captured here anyway).
    pub fn pause(&self) -> Result<(), PlatformError> {
        self.shared.paused.store(true, Ordering::Release);
        self.shared.clear();
        Ok(())
    }

    pub fn resume(&self) -> Result<(), PlatformError> {
        self.shared.paused.store(false, Ordering::Release);
        Ok(())
    }

    pub fn is_paused(&self) -> bool {
        self.shared.paused.load(Ordering::Acquire)
    }

    /// Always false: there are no hooks on this platform.
    pub fn is_installed(&self) -> bool {
        self.shared.installed.load(Ordering::Acquire)
    }

    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    pub fn reinstalls(&self) -> u64 {
        self.shared.reinstalls.load(Ordering::Relaxed)
    }

    pub fn shutdown(&self) {}
}

pub fn start_input(consumer: Thread) -> Result<(InputHandle, InputReceiver), PlatformError> {
    drop(consumer);
    let shared = Arc::new(InputShared::new());
    let rx = InputReceiver::new(shared.clone(), now_ms, |_| None, read_held);
    Ok((InputHandle { shared }, rx))
}

/// Control handle for the watcher. It keeps the event channel open.
#[derive(Clone, Debug)]
pub struct WatcherHandle {
    tx: Sender<ContextEvent>,
}

impl WatcherHandle {
    /// Sends the current layout again.
    pub fn refresh(&self) -> Result<(), PlatformError> {
        self.tx
            .send(ContextEvent::Layout(legends_snapshot()))
            .map_err(|_| PlatformError::NotRunning("watcher"))
    }

    pub fn is_running(&self) -> bool {
        true
    }

    /// False: there is no UI Automation here.
    pub fn uia_active(&self) -> bool {
        false
    }

    /// False: there are no session notifications here.
    pub fn session_notifications(&self) -> bool {
        false
    }

    pub fn shutdown(&self) {}
}

pub fn start_watcher(
    consumer: Thread,
) -> Result<(WatcherHandle, Receiver<ContextEvent>), PlatformError> {
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(ContextEvent::Foreground(ForegroundInfo::unknown()))
        .map_err(|_| PlatformError::NotRunning("watcher"))?;
    tx.send(ContextEvent::Layout(legends_snapshot()))
        .map_err(|_| PlatformError::NotRunning("watcher"))?;
    tx.send(ContextEvent::Password { active: false })
        .map_err(|_| PlatformError::NotRunning("watcher"))?;
    consumer.unpark();
    Ok((WatcherHandle { tx }, rx))
}

/// No monitors are known here.
pub fn monitors() -> Result<Vec<Monitor>, PlatformError> {
    Ok(Vec::new())
}

/// The US layout.
pub fn legends_snapshot() -> Arc<LegendTable> {
    static US: OnceLock<Arc<LegendTable>> = OnceLock::new();
    US.get_or_init(|| Arc::new(LegendTable::us())).clone()
}

/// Never readable here.
pub fn read_modifiers() -> Option<ModifierSnapshot> {
    None
}

fn read_held() -> Option<Vec<(Mod, Side)>> {
    None
}

pub fn foreground_process() -> ForegroundInfo {
    ForegroundInfo::unknown()
}

pub fn install_overlay_guard<R: tauri::Runtime>(
    _window: &tauri::WebviewWindow<R>,
) -> Result<(), PlatformError> {
    Ok(())
}

pub fn reassert_topmost<R: tauri::Runtime>(
    _window: &tauri::WebviewWindow<R>,
) -> Result<(), PlatformError> {
    Ok(())
}

pub fn set_capture_excluded<R: tauri::Runtime>(
    _window: &tauri::WebviewWindow<R>,
    _excluded: bool,
) -> Result<bool, PlatformError> {
    Ok(false)
}

pub fn capture_exclusion_supported() -> bool {
    false
}

pub fn window_scale<R: tauri::Runtime>(_window: &tauri::WebviewWindow<R>) -> Option<f64> {
    None
}

/// Refused: without DPAPI a secret could only be stored in plain text.
pub fn dpapi_protect(_plain: &str) -> Result<String, PlatformError> {
    Err(PlatformError::Unsupported)
}

pub fn dpapi_unprotect(_blob_b64: &str) -> Result<String, PlatformError> {
    Err(PlatformError::Unsupported)
}

pub fn os_build() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic() {
        let a = now_ms();
        let b = now_ms();
        assert!(b >= a);
    }

    #[test]
    fn input_stub_is_inert_but_consistent() {
        let (h, mut rx) = start_input(std::thread::current()).expect("stub input");
        assert!(!h.is_installed());
        assert!(!h.is_paused());
        h.pause().expect("pause");
        assert!(h.is_paused());
        h.resume().expect("resume");
        assert!(!h.is_paused());
        assert_eq!(h.dropped(), 0);
        assert_eq!(h.reinstalls(), 0);
        let mut out = Vec::new();
        assert_eq!(rx.drain_into(&mut out), 0);
        assert_eq!(rx.dropped(), 0);
        h.shutdown();
    }

    #[test]
    fn watcher_stub_reports_initial_state() {
        let (h, rx) = start_watcher(std::thread::current()).expect("stub watcher");
        assert!(
            matches!(rx.try_recv(), Ok(ContextEvent::Foreground(f)) if f == ForegroundInfo::unknown())
        );
        match rx.try_recv() {
            Ok(ContextEvent::Layout(t)) => assert_eq!(t.layout(), "00000409"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            rx.try_recv(),
            Ok(ContextEvent::Password { active: false })
        ));
        assert!(rx.try_recv().is_err());
        h.refresh().expect("refresh");
        assert!(matches!(rx.try_recv(), Ok(ContextEvent::Layout(_))));
        assert!(h.is_running());
        assert!(!h.uia_active());
        assert!(!h.session_notifications());
        h.shutdown();
    }

    #[test]
    fn everything_else_is_safe_and_honest() {
        assert_eq!(monitors().expect("monitors"), Vec::new());
        assert!(read_modifiers().is_none());
        assert_eq!(foreground_process(), ForegroundInfo::unknown());
        assert!(!capture_exclusion_supported());
        assert_eq!(os_build(), None);
        assert!(matches!(
            dpapi_protect("x"),
            Err(PlatformError::Unsupported)
        ));
        assert!(matches!(
            dpapi_unprotect("eA=="),
            Err(PlatformError::Unsupported)
        ));
        let a = legends_snapshot();
        let b = legends_snapshot();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(a.wire().legends.get("sc:1E").map(String::as_str), Some("A"));
    }
}
