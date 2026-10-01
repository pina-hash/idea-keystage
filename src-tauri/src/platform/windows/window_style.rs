//! The overlay window's styles, z-order, capture exclusion and DPI.
//!
//! tao rewrites `GWL_EXSTYLE` from its own flags on every show, hide,
//! click-through or topmost change, wiping `WS_EX_TOOLWINDOW` and putting
//! `WS_EX_APPWINDOW` back (`.work/research/stack.md` A.3). The guard
//! subclasses the window and fixes every proposed extended style in
//! `WM_STYLECHANGING` ("A pointer to a STYLESTRUCT structure that contains
//! the proposed new styles for the window. An application can examine the
//! styles and, if necessary, change them", WM_STYLECHANGING, read
//! 2026-10-01).
//!
//! **Main thread only**: "You cannot use the subclassing helper functions
//! to subclass a window across threads" (SetWindowSubclass, read
//! 2026-10-01). Every public function here checks or documents that. The
//! one exception is [`reassert_overlay_topmost_async`], which the watcher
//! thread calls on every foreground window change: it uses
//! `SWP_ASYNCWINDOWPOS` ("If the calling thread and the thread that owns the
//! window are attached to different input queues, the system posts the
//! request to the thread that owns the window. This prevents the calling
//! thread from blocking its execution while other threads process the
//! request", SetWindowPos), so it never waits for the main thread.

use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GetWindowThreadProcessId, SetWindowDisplayAffinity, SetWindowLongPtrW,
    SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SET_WINDOW_POS_FLAGS, STYLESTRUCT, SWP_ASYNCWINDOWPOS,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WDA_EXCLUDEFROMCAPTURE,
    WDA_NONE, WM_NCDESTROY, WM_STYLECHANGING,
};

use super::super::display::{
    capture_exclusion_supported_on, ex_style, force_overlay_ex_style, scale_from_dpi,
};
use super::super::PlatformError;
use super::os;

const _: () = {
    use windows::Win32::UI::WindowsAndMessaging as wm;
    assert!(ex_style::WS_EX_TRANSPARENT == wm::WS_EX_TRANSPARENT.0);
    assert!(ex_style::WS_EX_TOOLWINDOW == wm::WS_EX_TOOLWINDOW.0);
    assert!(ex_style::WS_EX_APPWINDOW == wm::WS_EX_APPWINDOW.0);
    assert!(ex_style::WS_EX_LAYERED == wm::WS_EX_LAYERED.0);
    assert!(ex_style::WS_EX_NOACTIVATE == wm::WS_EX_NOACTIVATE.0);
};

/// Our subclass id ("KSOV").
const SUBCLASS_ID: usize = 0x4B53_4F56;

/// The window the guard was last installed on (the overlay), 0 when none.
/// Cleared when that window is destroyed.
static OVERLAY: AtomicIsize = AtomicIsize::new(0);

/// `SetWindowPos` flags that put a window at the top of the topmost group
/// without activating, moving or sizing it.
const TOPMOST_FLAGS: SET_WINDOW_POS_FLAGS =
    SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0 | SWP_NOMOVE.0 | SWP_NOSIZE.0);

fn on_owner_thread(hwnd: HWND) -> bool {
    // SAFETY: plain calls.
    unsafe { GetWindowThreadProcessId(hwnd, None) == GetCurrentThreadId() }
}

unsafe extern "system" fn guard_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _ref_data: usize,
) -> LRESULT {
    if msg == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 {
        let ss = lparam.0 as *mut STYLESTRUCT;
        if !ss.is_null() {
            // SAFETY: for WM_STYLECHANGING, lParam points to the STYLESTRUCT
            // the system is about to apply.
            unsafe { (*ss).styleNew = force_overlay_ex_style((*ss).styleNew) };
        }
    } else if msg == WM_NCDESTROY {
        // The window is going away: forget it (unless a newer overlay took
        // its place) and take our subclass out of its chain.
        let _ = OVERLAY.compare_exchange(hwnd.0 as isize, 0, Ordering::AcqRel, Ordering::Acquire);
        // SAFETY: removing our own subclass on the window's thread.
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(guard_proc), id) };
    }
    // SAFETY: passes the message on down the subclass chain.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Installs the style guard and applies the overlay style once
/// (`SetWindowLongPtrW`, then `SetWindowPos` with `SWP_FRAMECHANGED |
/// SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER`, as "If you
/// have changed certain window data using SetWindowLong, you must call
/// SetWindowPos for the changes to take effect"). Installing twice is
/// harmless.
///
/// Main thread only (the window's thread); returns
/// [`PlatformError::WrongThread`] otherwise.
pub fn install_overlay_guard(hwnd: HWND) -> Result<(), PlatformError> {
    if hwnd.0.is_null() {
        return Err(PlatformError::InvalidInput(
            "the overlay window handle is null",
        ));
    }
    if !on_owner_thread(hwnd) {
        return Err(PlatformError::WrongThread("install_overlay_guard"));
    }
    // SAFETY: subclassing a window of this thread with a procedure that
    // lives for the whole process.
    if !unsafe { SetWindowSubclass(hwnd, Some(guard_proc), SUBCLASS_ID, 0) }.as_bool() {
        return Err(PlatformError::last_os_error("SetWindowSubclass"));
    }
    OVERLAY.store(hwnd.0 as isize, Ordering::Release);
    // SAFETY: plain reads and writes of this thread's window.
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let wanted = force_overlay_ex_style(current);
        if wanted != current {
            // The guard sees this change too and keeps it as it is.
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted as i32 as isize);
        }
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        )
        .map_err(|e| PlatformError::os("SetWindowPos(SWP_FRAMECHANGED)", &e))?;
    }
    Ok(())
}

/// Moves the window to the top of the topmost group without activating it.
/// Main thread (a cross-thread call would wait for the main thread).
pub fn reassert_topmost(hwnd: HWND) -> Result<(), PlatformError> {
    if hwnd.0.is_null() {
        return Err(PlatformError::InvalidInput(
            "the overlay window handle is null",
        ));
    }
    // SAFETY: plain call on our own window.
    unsafe { SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, TOPMOST_FLAGS) }
        .map_err(|e| PlatformError::os("SetWindowPos(HWND_TOPMOST)", &e))
}

/// Puts the guarded overlay back on top of the topmost group, from any
/// thread, without waiting: the request is posted to the window's thread
/// (`SWP_ASYNCWINDOWPOS`). The watcher calls it on every foreground window
/// change (ARCHITECTURE.md section 6), including changes between two
/// windows of one process, which `ContextEvent::Foreground` does not
/// report. Does nothing while no overlay is guarded. Returns whether a
/// request was made.
pub(crate) fn reassert_overlay_topmost_async() -> bool {
    let raw = OVERLAY.load(Ordering::Acquire);
    if raw == 0 {
        return false;
    }
    let hwnd = HWND(raw as *mut std::ffi::c_void);
    let mut pid = 0u32;
    // SAFETY: plain calls; a handle that is no longer a window fails them.
    unsafe {
        // Only a live window of this process: never touch a window that
        // reused the value after ours was destroyed.
        if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 || pid != GetCurrentProcessId() {
            return false;
        }
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            TOPMOST_FLAGS | SWP_ASYNCWINDOWPOS,
        )
        .is_ok()
    }
}

/// Excludes the window from capture on build 19041 and later; elsewhere,
/// and when `excluded` is false, sets `WDA_NONE`. Returns whether the window
/// is now excluded. Main thread.
pub fn set_capture_excluded(hwnd: HWND, excluded: bool) -> Result<bool, PlatformError> {
    if hwnd.0.is_null() {
        return Err(PlatformError::InvalidInput(
            "the overlay window handle is null",
        ));
    }
    let apply = excluded && capture_exclusion_supported_on(os::build());
    let affinity = if apply {
        WDA_EXCLUDEFROMCAPTURE
    } else {
        WDA_NONE
    };
    // SAFETY: plain call on our own window.
    unsafe { SetWindowDisplayAffinity(hwnd, affinity) }
        .map_err(|e| PlatformError::os("SetWindowDisplayAffinity", &e))?;
    Ok(apply)
}

/// `GetDpiForWindow` as a scale factor, `None` for an invalid window.
pub fn window_scale(hwnd: HWND) -> Option<f64> {
    if hwnd.0.is_null() {
        return None;
    }
    // SAFETY: plain call.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    (dpi != 0).then(|| scale_from_dpi(dpi))
}
