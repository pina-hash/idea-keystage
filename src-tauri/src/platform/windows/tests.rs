//! Windows-only tests of the platform layer against the real APIs: DPAPI,
//! the clock, monitors, the overlay style guard, and the life cycle of the
//! input hooks and the watcher (including an injected key travelling
//! through the low-level hook into the receiver).
//!
//! They run with `cargo test -p idea-keystage` on Windows. The key injected
//! is F24, which no common app binds. A few calls that Wine does not
//! implement are checked only on real Windows ([`running_under_wine`]).

use std::thread;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use keystage_core::keys::vk;
use keystage_core::InputEvent;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, RegisterClassExW,
    SetWindowLongPtrW, GWL_EXSTYLE, WNDCLASSEXW, WS_EX_APPWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::super::display::{OVERLAY_EX_STYLE_OFF, OVERLAY_EX_STYLE_ON};
use super::super::{ContextEvent, PlatformError};
use super::os::running_under_wine;
use super::*;

#[test]
fn clock_and_build() {
    assert!(os::build().is_some_and(|b| b > 0));
    let a = now_ms();
    let b = now_ms();
    assert!(b >= a);
    let t32 = os::tick_count();
    let t64 = os::tick_count64();
    assert!(
        (t64 as u32).wrapping_sub(t32) < 1_000,
        "GetTickCount is the low half of GetTickCount64"
    );
    assert_eq!(
        capture_exclusion_supported(),
        os::build().is_some_and(|b| b >= 19041)
    );
}

#[test]
fn dpapi_round_trip_and_tampering() {
    let blob = dpapi_protect("obs password 123").expect("protect");
    assert!(!blob.contains("obs password"));
    assert_eq!(
        dpapi_unprotect(&blob).expect("unprotect"),
        "obs password 123"
    );
    // Two protections of the same secret differ (random salt) but both open.
    let blob2 = dpapi_protect("obs password 123").expect("protect");
    assert_eq!(
        dpapi_unprotect(&blob2).expect("unprotect"),
        "obs password 123"
    );
    // A truncated blob never yields the secret.
    let raw = STANDARD.decode(&blob).expect("base64");
    let truncated = STANDARD.encode(&raw[..raw.len() / 2]);
    assert_ne!(
        dpapi_unprotect(&truncated).ok().as_deref(),
        Some("obs password 123")
    );
    if !running_under_wine() {
        // Windows checks the blob's MAC ("adds a Message Authentication
        // Code ... to guard against data tampering"); Wine does not.
        let mut flipped = raw.clone();
        if let Some(last) = flipped.last_mut() {
            *last ^= 0xFF;
        }
        assert_ne!(
            dpapi_unprotect(&STANDARD.encode(&flipped)).ok().as_deref(),
            Some("obs password 123")
        );
    }
    assert!(matches!(
        dpapi_unprotect("not a blob"),
        Err(PlatformError::InvalidData)
    ));
    assert!(matches!(
        dpapi_protect(""),
        Err(PlatformError::InvalidInput(_))
    ));
}

#[test]
fn monitors_are_well_formed() {
    let list = monitors().expect("monitors");
    let mut names = std::collections::HashSet::new();
    for m in &list {
        assert!(m.id.starts_with(r"\\.\"), "{}", m.id);
        assert!(m.width > 0 && m.height > 0);
        assert!(m.scale >= 0.5 && m.scale <= 8.0, "{}", m.scale);
        assert!(!m.name.trim().is_empty());
        assert!(names.insert(m.name.clone()), "names are distinct");
    }
    assert!(list.iter().filter(|m| m.primary).count() <= 1);
    if let Some(first) = list.first() {
        if list.iter().any(|m| m.primary) {
            assert!(first.primary, "the primary monitor comes first");
        }
    }
}

unsafe extern "system" fn plain_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: default processing.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn ex_style(hwnd: HWND) -> u32 {
    // SAFETY: plain read.
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

#[test]
fn overlay_guard_keeps_the_overlay_styles() {
    // SAFETY: creating, styling and destroying a window on this thread.
    unsafe {
        let module = GetModuleHandleW(None).expect("module");
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(plain_wndproc),
            hInstance: module.into(),
            lpszClassName: w!("IDEAKeystageGuardTest"),
            ..Default::default()
        };
        assert_ne!(RegisterClassExW(&class), 0);
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            w!("IDEAKeystageGuardTest"),
            w!("guard test"),
            WS_POPUP,
            0,
            0,
            40,
            30,
            None,
            None,
            Some(module.into()),
            None,
        )
        .expect("window");

        window_style::install_overlay_guard(hwnd).expect("guard");
        let ex = ex_style(hwnd);
        assert_eq!(ex & OVERLAY_EX_STYLE_ON, OVERLAY_EX_STYLE_ON);
        assert_eq!(ex & OVERLAY_EX_STYLE_OFF, 0);

        // What tao does on every state change: rewrite GWL_EXSTYLE.
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, WS_EX_APPWINDOW.0 as isize);
        let ex = ex_style(hwnd);
        assert_eq!(ex & OVERLAY_EX_STYLE_ON, OVERLAY_EX_STYLE_ON, "{ex:#x}");
        assert_eq!(ex & OVERLAY_EX_STYLE_OFF, 0, "{ex:#x}");

        // Installing again is harmless.
        window_style::install_overlay_guard(hwnd).expect("second install");

        // Another thread is refused.
        let raw = hwnd.0 as isize;
        let other = thread::spawn(move || {
            window_style::install_overlay_guard(HWND(raw as *mut std::ffi::c_void))
        })
        .join()
        .expect("thread");
        assert!(matches!(other, Err(PlatformError::WrongThread(_))));

        window_style::reassert_topmost(hwnd).expect("topmost");
        assert_ne!(ex_style(hwnd) & WS_EX_TOPMOST.0, 0);
        // The watcher's variant targets the guarded window from any thread.
        assert!(window_style::reassert_overlay_topmost_async());
        let from_other_thread = thread::spawn(window_style::reassert_overlay_topmost_async)
            .join()
            .expect("thread");
        assert!(from_other_thread);
        assert!(window_style::window_scale(hwnd).is_some_and(|s| s > 0.0));

        if !running_under_wine() {
            assert!(!window_style::set_capture_excluded(hwnd, false).expect("affinity"));
            assert_eq!(
                window_style::set_capture_excluded(hwnd, true).expect("affinity"),
                capture_exclusion_supported()
            );
        }

        // WM_NCDESTROY takes the subclass out again and forgets the window.
        DestroyWindow(hwnd).expect("destroy");
    }
    assert!(!window_style::reassert_overlay_topmost_async());
    assert!(matches!(
        window_style::install_overlay_guard(HWND(std::ptr::null_mut())),
        Err(PlatformError::InvalidInput(_))
    ));
    assert_eq!(window_style::window_scale(HWND(std::ptr::null_mut())), None);
}

fn send_key(code: u16, up: bool) {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(code),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    // SAFETY: one fully initialized INPUT.
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    assert_eq!(sent, 1, "SendInput");
}

/// Drains until `want` F24 events arrived or the timeout passed.
fn collect_f24(
    rx: &mut crate::platform::InputReceiver,
    want: usize,
    timeout: Duration,
) -> Vec<keystage_core::KeyInput> {
    let deadline = Instant::now() + timeout;
    let mut keys = Vec::new();
    let mut buf = Vec::new();
    while Instant::now() < deadline && keys.len() < want {
        thread::park_timeout(Duration::from_millis(20));
        buf.clear();
        rx.drain_into(&mut buf);
        keys.extend(buf.iter().filter_map(|e| match e {
            InputEvent::Key(k) if k.vk == vk::F24 => Some(*k),
            _ => None,
        }));
    }
    keys
}

fn wait_for(cond: impl Fn() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    cond()
}

#[test]
fn input_hooks_life_cycle_with_an_injected_key() {
    let (handle, mut rx) = start_input(thread::current()).expect("hooks");
    assert!(handle.is_installed());
    assert!(!handle.is_paused());
    assert!(matches!(
        start_input(thread::current()),
        Err(PlatformError::AlreadyRunning(_))
    ));

    send_key(vk::F24, false);
    send_key(vk::F24, true);
    let keys = collect_f24(&mut rx, 2, Duration::from_secs(3));
    assert_eq!(keys.len(), 2, "both F24 events through the hook");
    assert!(keys[0].down && !keys[1].down);
    assert!(keys[0].injected && keys[1].injected);
    assert!(keys[1].t_ms >= keys[0].t_ms);
    assert!(now_ms().saturating_sub(keys[0].t_ms) < 10_000, "same clock");
    assert_eq!(handle.dropped(), 0);

    // Paused: hooks removed, nothing recorded.
    handle.pause().expect("pause");
    assert!(handle.is_paused());
    assert!(wait_for(|| !handle.is_installed(), Duration::from_secs(2)));
    send_key(vk::F24, false);
    send_key(vk::F24, true);
    assert!(collect_f24(&mut rx, 1, Duration::from_millis(300)).is_empty());

    // Resumed: hooks back, events flow again.
    handle.resume().expect("resume");
    assert!(wait_for(|| handle.is_installed(), Duration::from_secs(2)));
    send_key(vk::F24, false);
    send_key(vk::F24, true);
    assert_eq!(collect_f24(&mut rx, 2, Duration::from_secs(3)).len(), 2);

    // A stall long enough for Windows to drop a hook: the hook thread notices
    // it on its own timer and reinstalls both hooks.
    let before = handle.reinstalls();
    handle.stall_hook_thread_for_test(400).expect("stall");
    assert!(
        wait_for(|| handle.reinstalls() > before, Duration::from_secs(3)),
        "reinstalled after the stall"
    );
    assert!(wait_for(|| handle.is_installed(), Duration::from_secs(2)));
    send_key(vk::F24, false);
    send_key(vk::F24, true);
    assert_eq!(collect_f24(&mut rx, 2, Duration::from_secs(3)).len(), 2);

    handle.shutdown();
    assert!(!handle.is_installed());
    handle.shutdown(); // idempotent
    assert!(matches!(handle.pause(), Err(PlatformError::Os { .. })));

    // Restart with the same consumer; a different consumer is refused.
    let (again, _rx2) = start_input(thread::current()).expect("restart");
    assert!(again.is_installed());
    let other = thread::spawn(|| start_input(thread::current()).err())
        .join()
        .expect("thread");
    assert!(matches!(other, Some(PlatformError::InvalidInput(_))));
    drop(again); // the last clone stops the hooks
}

#[test]
fn watcher_reports_state_and_stops() {
    let (handle, rx) = start_watcher(thread::current()).expect("watcher");
    assert!(handle.is_running());
    assert!(matches!(
        start_watcher(thread::current()),
        Err(PlatformError::AlreadyRunning(_))
    ));

    let (mut foreground, mut layout, mut password) = (false, false, false);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && !(foreground && layout && password) {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(ContextEvent::Foreground(_)) => foreground = true,
            Ok(ContextEvent::Layout(t)) => {
                assert!(!t.wire().legends.is_empty());
                layout = true;
            }
            Ok(ContextEvent::Password { .. }) => password = true,
            Ok(_) | Err(_) => {}
        }
    }
    assert!(foreground && layout && password, "initial state");
    if !running_under_wine() {
        // The password guard depends on it: UI Automation must subscribe.
        assert!(
            wait_for(|| handle.uia_active(), Duration::from_secs(5)),
            "UI Automation focus handler"
        );
    }

    // A refresh re-sends the foreground.
    handle.refresh().expect("refresh");
    let got = (0..60).any(|_| {
        matches!(
            rx.recv_timeout(Duration::from_millis(50)),
            Ok(ContextEvent::Foreground(_))
        )
    });
    assert!(got, "foreground after refresh");

    handle.shutdown();
    assert!(!handle.is_running());
    assert!(matches!(
        handle.refresh(),
        Err(PlatformError::NotRunning(_))
    ));
    handle.shutdown(); // idempotent

    // It can start again.
    let (again, _rx2) = start_watcher(thread::current()).expect("restart");
    assert!(again.is_running());
    again.shutdown();
}

#[test]
fn snapshots_never_fail() {
    let t = legends_snapshot();
    assert!(!t.wire().legends.is_empty());
    let _ = foreground_process();
    // Readable unless the test machine's foreground is elevated or locked.
    if let Some(m) = read_modifiers() {
        assert!(m.held.len() <= 8);
    }
}
