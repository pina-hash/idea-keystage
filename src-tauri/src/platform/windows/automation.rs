//! The connector's Windows calls (ARCHITECTURE.md section 14): `SendInput`
//! for the real cursor and keyboard, the cursor position and the virtual
//! screen, the top-level windows for clean takes (never their titles), the
//! foreground window's rectangle, and a hash of a small screen rectangle
//! for `wait_idle`.
//!
//! The process is per-monitor DPI aware v2 (its manifest), so every
//! coordinate here is physical pixels of the virtual screen.
//!
//! Threading: any thread except a hook procedure. Nothing here logs.

use std::ffi::c_void;

use keystage_automation::keys::Key;
use keystage_automation::motion::{to_absolute, Rect};
use keystage_automation::occlude::WindowInfo;
use keystage_automation::script::Button;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
    GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    GWL_EXSTYLE, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    WS_EX_TOPMOST,
};

use super::process;
use crate::platform::PlatformError;

fn send(inputs: &[INPUT]) -> Result<(), PlatformError> {
    // SAFETY: a slice of fully initialized INPUT structures and their size.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(PlatformError::Unsupported)
    }
}

fn mouse(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// The virtual screen, physical pixels.
pub fn virtual_screen() -> Rect {
    // SAFETY: plain metric reads.
    unsafe {
        Rect {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            width: GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) as u32,
            height: GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) as u32,
        }
    }
}

pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    // SAFETY: `p` receives the position.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((p.x, p.y))
}

/// Moves the cursor to a physical virtual-screen pixel.
pub fn move_to(x: i32, y: i32, screen: &Rect) -> Result<(), PlatformError> {
    let (ax, ay) = to_absolute(x, y, screen);
    send(&[mouse(
        ax,
        ay,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )])
}

pub fn button(b: Button, down: bool) -> Result<(), PlatformError> {
    let f = match (b, down) {
        (Button::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (Button::Left, false) => MOUSEEVENTF_LEFTUP,
        (Button::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (Button::Right, false) => MOUSEEVENTF_RIGHTUP,
        (Button::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (Button::Middle, false) => MOUSEEVENTF_MIDDLEUP,
    };
    send(&[mouse(0, 0, 0, f)])
}

/// One wheel notch: `delta` +120 is up (or right with `horizontal`).
pub fn wheel(delta: i32, horizontal: bool) -> Result<(), PlatformError> {
    let f = if horizontal {
        MOUSEEVENTF_HWHEEL
    } else {
        MOUSEEVENTF_WHEEL
    };
    send(&[mouse(0, 0, delta as u32, f)])
}

pub fn key(k: Key, down: bool) -> Result<(), PlatformError> {
    let up = if down {
        KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };
    let ki = match k {
        Key::Vk { vk, extended } => KEYBDINPUT {
            wVk: VIRTUAL_KEY(vk),
            wScan: 0,
            dwFlags: up
                | if extended {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
            time: 0,
            dwExtraInfo: 0,
        },
        Key::Unicode(u) => KEYBDINPUT {
            wVk: VIRTUAL_KEY(0),
            wScan: u,
            dwFlags: up | KEYEVENTF_UNICODE,
            time: 0,
            dwExtraInfo: 0,
        },
    };
    send(&[INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    }])
}

fn rect_of(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: `r` receives the rectangle.
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    Some(Rect {
        x: r.left,
        y: r.top,
        width: (r.right - r.left).max(0) as u32,
        height: (r.bottom - r.top).max(0) as u32,
    })
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    // SAFETY: the buffer outlives the call.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn cloaked(hwnd: HWND) -> bool {
    let mut v: u32 = 0;
    // SAFETY: `v` is a 4-byte buffer for the DWORD attribute.
    unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut v as *mut u32 as *mut c_void, 4) }
        .is_ok()
        && v != 0
}

struct Walk {
    out: Vec<HWND>,
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `Walk` that `EnumWindows` was given, alive for
    // the whole call.
    let walk = unsafe { &mut *(lparam.0 as *mut Walk) };
    if walk.out.len() < 4096 {
        walk.out.push(hwnd);
    }
    BOOL(1)
}

/// The visible top-level windows in z-order (0 is the top), described
/// without their titles.
pub fn top_level_windows() -> Vec<WindowInfo> {
    let mut walk = Walk { out: Vec::new() };
    // SAFETY: `collect` only pushes into `walk`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut walk as *mut Walk as isize)) };
    // SAFETY: no preconditions.
    let fg = unsafe { GetForegroundWindow() };
    let me = std::process::id();
    let mut out = Vec::new();
    for (z, hwnd) in walk.out.into_iter().enumerate() {
        // SAFETY: plain queries on a window handle (a closed one fails).
        let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        if !visible && hwnd != fg {
            continue;
        }
        let Some(rect) = rect_of(hwnd) else { continue };
        let mut pid = 0u32;
        // SAFETY: `pid` receives the process id.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let info = process::foreground_info(hwnd, None);
        // SAFETY: plain style read.
        let ex = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        out.push(WindowInfo {
            z: z as u32,
            process: info.exe.unwrap_or_default(),
            class: class_of(hwnd),
            rect,
            topmost: ex & WS_EX_TOPMOST.0 != 0,
            visible,
            cloaked: cloaked(hwnd),
            // SAFETY: plain query.
            minimized: unsafe { IsIconic(hwnd) }.as_bool(),
            own: pid == me,
            foreground: hwnd == fg,
        });
    }
    out
}

/// The foreground window's process name and rectangle (never its title).
pub fn foreground_window() -> Option<(String, Rect)> {
    // SAFETY: no preconditions.
    let fg = unsafe { GetForegroundWindow() };
    if fg.0.is_null() {
        return None;
    }
    let info = process::foreground_info(fg, None);
    Some((info.exe.unwrap_or_default(), rect_of(fg)?))
}

/// A hash of the screen pixels in `r` (at most 256 by 256), to see whether
/// that part of the screen is still changing. The pixels are hashed and
/// dropped at once; nothing is kept or saved.
pub fn region_hash(r: &Rect) -> Option<u64> {
    let w = r.width.clamp(1, 256) as i32;
    let h = r.height.clamp(1, 256) as i32;
    // SAFETY: the screen DC, a memory DC and a bitmap are created, used and
    // released in order; `buf` is large enough for 32-bit rows of `w` by `h`.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        let ok = BitBlt(mem, 0, 0, w, h, Some(screen), r.x, r.y, SRCCOPY).is_ok();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        SelectObject(mem, old);
        let lines = if ok {
            GetDIBits(
                mem,
                bmp,
                0,
                h as u32,
                Some(buf.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if lines != h {
            return None;
        }
        // FNV-1a.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in &buf {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Some(hash)
    }
}
