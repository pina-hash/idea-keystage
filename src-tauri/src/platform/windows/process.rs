//! The foreground process, its elevation, the held modifiers, and the
//! classic Edit control password check.
//!
//! Sources (read 2026-10-01): GetWindowThreadProcessId, OpenProcess,
//! QueryFullProcessImageNameW, Process Security and Access Rights,
//! OpenProcessToken, TOKEN_ELEVATION, GetExitCodeProcess, GetGUIThreadInfo,
//! Edit Control Styles, GetAsyncKeyState (`.work/research/windows.md`
//! sections 2.4, 3 and 4.3).

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess, OpenProcess, OpenProcessToken,
    QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW,
    GetWindowThreadProcessId, GUITHREADINFO, GWL_STYLE,
};

use keystage_core::keys::vk;

use super::super::context::{self, ES_PASSWORD};
use super::super::convert;
use super::super::display::wide_to_string;
use super::super::{ForegroundInfo, ModifierSnapshot, Verdict};

const _: () = assert!(ES_PASSWORD as i32 == windows::Win32::UI::WindowsAndMessaging::ES_PASSWORD);

/// `STILL_ACTIVE`: what GetExitCodeProcess reports for a running process.
const STILL_ACTIVE: u32 = 259;

/// How many processes the watcher keeps open.
const CACHE_LIMIT: usize = 32;

/// An open process handle, closed on drop.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: a handle we opened and own.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

fn open_process(pid: u32) -> Option<OwnedHandle> {
    // SAFETY: plain call; the handle is owned by the returned value.
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .ok()
        .map(OwnedHandle)
}

/// The image path of a process, Win32 format.
fn image_path(process: &OwnedHandle) -> Option<String> {
    for cap in [1024usize, 32_768] {
        let mut buf = vec![0u16; cap];
        let mut len = cap as u32;
        // SAFETY: `buf` holds `len` UTF-16 units; `len` receives the count.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
        }
        .is_ok();
        if ok {
            let n = (len as usize).min(buf.len());
            return Some(String::from_utf16_lossy(buf.get(..n).unwrap_or(&[])));
        }
    }
    None
}

/// Whether a process token is elevated. `None` when the token cannot be read.
fn token_elevated(process: HANDLE) -> Option<bool> {
    let mut token = HANDLE::default();
    // SAFETY: `token` receives a handle we close below.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let token = OwnedHandle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    // SAFETY: the buffer is a TOKEN_ELEVATION of the size passed.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    }
    .is_ok();
    ok.then_some(elevation.TokenIsElevated != 0)
}

/// Whether IDEA Keystage itself runs elevated.
pub fn self_elevated() -> Option<bool> {
    static SELF: OnceLock<Option<bool>> = OnceLock::new();
    // SAFETY: the pseudo handle needs no closing.
    *SELF.get_or_init(|| token_elevated(unsafe { GetCurrentProcess() }))
}

fn own_pid() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentProcessId() }
}

fn own_exe() -> Option<String> {
    static EXE: OnceLock<Option<String>> = OnceLock::new();
    EXE.get_or_init(|| {
        let path = std::env::current_exe().ok()?;
        Some(context::exe_file_name(&path.to_string_lossy()).to_string())
    })
    .clone()
}

fn is_running(process: &OwnedHandle) -> bool {
    let mut code = 0u32;
    // SAFETY: `code` receives the exit status.
    unsafe { GetExitCodeProcess(process.0, &mut code) }.is_ok() && code == STILL_ACTIVE
}

struct Cached {
    pid: u32,
    process: OwnedHandle,
    exe: String,
    elevated: Option<bool>,
    used: u64,
}

/// Foreground processes the watcher has seen, keyed by process id.
///
/// An entry keeps its process handle open and is used only while that
/// process is still running ("The identifier is valid from the time the
/// process is created until the process has been terminated", Process
/// Handles and Identifiers), so a reused process id never returns a stale
/// name.
#[derive(Default)]
pub struct ProcessCache {
    entries: Vec<Cached>,
    clock: u64,
}

impl ProcessCache {
    pub fn new() -> ProcessCache {
        ProcessCache::default()
    }

    /// The executable name and elevation of `pid`, or `None` if the process
    /// cannot be opened or named.
    fn lookup(&mut self, pid: u32) -> Option<(String, Option<bool>)> {
        self.clock += 1;
        if let Some(i) = self.entries.iter().position(|e| e.pid == pid) {
            if self.entries.get(i).is_some_and(|e| is_running(&e.process)) {
                let e = self.entries.get_mut(i)?;
                e.used = self.clock;
                return Some((e.exe.clone(), e.elevated));
            }
            self.entries.swap_remove(i);
        }
        let process = open_process(pid)?;
        let path = image_path(&process)?;
        let exe = context::exe_file_name(&path).to_string();
        let elevated = token_elevated(process.0);
        if self.entries.len() >= CACHE_LIMIT {
            if let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.used)
                .map(|(i, _)| i)
            {
                self.entries.swap_remove(oldest);
            }
        }
        self.entries.push(Cached {
            pid,
            process,
            exe: exe.clone(),
            elevated,
            used: self.clock,
        });
        Some((exe, elevated))
    }

    /// Closes the handles of processes that have exited.
    pub fn sweep(&mut self) {
        self.entries.retain(|e| is_running(&e.process));
    }
}

fn lookup_uncached(pid: u32) -> Option<(String, Option<bool>)> {
    let process = open_process(pid)?;
    let path = image_path(&process)?;
    Some((
        context::exe_file_name(&path).to_string(),
        token_elevated(process.0),
    ))
}

/// Identifies the process that owns `hwnd`.
pub fn foreground_info(hwnd: HWND, cache: Option<&mut ProcessCache>) -> ForegroundInfo {
    if hwnd.0.is_null() {
        return ForegroundInfo::unknown();
    }
    let mut pid = 0u32;
    // SAFETY: `pid` receives the process id.
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if tid == 0 || pid == 0 {
        return ForegroundInfo::unknown();
    }
    let me = own_pid();
    let found = if pid == me {
        own_exe().map(|exe| (exe, self_elevated()))
    } else {
        match cache {
            Some(c) => c.lookup(pid),
            None => lookup_uncached(pid),
        }
    };
    let (exe, elevated) = match found {
        Some((exe, elevated)) => (Some(exe), elevated),
        None => (None, None),
    };
    let kind = context::classify_foreground(pid, me, exe.is_some(), elevated, self_elevated());
    ForegroundInfo { kind, exe, pid }
}

/// The current foreground process, without a cache.
pub fn foreground_now() -> ForegroundInfo {
    // SAFETY: no preconditions.
    let hwnd = unsafe { GetForegroundWindow() };
    foreground_info(hwnd, None)
}

/// The modifiers held now. The caller decides whether the desktop is
/// readable (`super::desktop_readable`).
pub fn modifier_snapshot(t_ms: u64) -> ModifierSnapshot {
    let held = convert::held_modifiers(|code| {
        // SAFETY: plain call with a virtual-key code.
        convert::async_key_down(unsafe { GetAsyncKeyState(i32::from(code)) })
    });
    // GetKeyState reflects this thread's view; it seeds Caps Lock only, and
    // the processor then follows Caps Lock presses itself.
    // SAFETY: plain call with a virtual-key code.
    let caps = convert::key_toggled(unsafe { GetKeyState(i32::from(vk::CAPITAL)) });
    ModifierSnapshot {
        held,
        caps_lock: Some(caps),
        t_ms,
    }
}

/// The modifiers held now, or `None` when the desktop is not readable
/// (elevated or unknown foreground, locked session). Used by the input
/// receiver after the hooks were (re)installed.
pub fn held_modifiers_if_readable() -> Option<Vec<(keystage_core::Mod, keystage_core::Side)>> {
    super::desktop_readable().then(|| modifier_snapshot(0).held)
}

/// The classic Win32 password check on the foreground thread's focus
/// window: an Edit-based control with `ES_PASSWORD`. Reads window-manager
/// state only, so it sends nothing to the target and cannot hang.
pub fn win32_password_verdict() -> Verdict {
    let mut gti = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `gti` is a valid GUITHREADINFO with its size set; thread 0 is
    // the foreground thread.
    if unsafe { GetGUIThreadInfo(0, &mut gti) }.is_err() {
        return Verdict::Unknown;
    }
    let focus = gti.hwndFocus;
    if focus.0.is_null() {
        return Verdict::NotPassword;
    }
    let mut class = [0u16; 128];
    // SAFETY: `class` is a writable buffer.
    let n = unsafe { GetClassNameW(focus, &mut class) };
    if n <= 0 {
        return Verdict::Unknown;
    }
    let class = wide_to_string(class.get(..n as usize).unwrap_or(&[]));
    // SAFETY: plain read of the window's style bits.
    let style = unsafe { GetWindowLongPtrW(focus, GWL_STYLE) } as u32;
    context::win32_verdict(&class, style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_is_named_and_cached() {
        let me = own_pid();
        let expected = own_exe().expect("own exe name");
        let mut cache = ProcessCache::new();
        let (exe, elevated) = cache.lookup(me).expect("open own process");
        assert_eq!(exe, expected);
        assert_eq!(elevated, self_elevated());
        assert_eq!(cache.entries.len(), 1);
        // A second lookup is served from the cache.
        let (again, _) = cache.lookup(me).expect("cached");
        assert_eq!(again, expected);
        assert_eq!(cache.entries.len(), 1);
        // We are running, so a sweep keeps the entry.
        cache.sweep();
        assert_eq!(cache.entries.len(), 1);
        // Process id 0 (the idle process) cannot be opened.
        assert!(cache.lookup(0).is_none());
        assert!(self_elevated().is_some(), "our own token is readable");
    }

    #[test]
    fn foreground_queries_never_fail() {
        let null = foreground_info(HWND(std::ptr::null_mut()), None);
        assert_eq!(null, ForegroundInfo::unknown());
        let fg = foreground_now();
        if fg.kind == context::ForegroundKind::App {
            assert!(fg.exe.is_some());
            assert_ne!(fg.pid, 0);
        }
        // Whatever has the focus, the check returns a verdict.
        let _ = win32_password_verdict();
        let snap = modifier_snapshot(42);
        assert_eq!(snap.t_ms, 42);
        assert!(snap.held.len() <= 8);
        assert!(snap.caps_lock.is_some());
    }
}
