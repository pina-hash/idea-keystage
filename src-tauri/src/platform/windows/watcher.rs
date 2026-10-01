//! The watcher thread: foreground process, keyboard layout, password focus,
//! session lock and unlock.
//!
//! Microsoft's constraints (read 2026-10-01; `.work/research/windows.md`
//! sections 2 to 5):
//!
//! * SetWinEventHook: "The client thread that calls SetWinEventHook must
//!   have a message loop", out-of-context events are "delivered on the same
//!   thread that called SetWinEventHook", and `UnhookWinEvent` must be
//!   called from that thread. The hooks use `WINEVENT_OUTOFCONTEXT |
//!   WINEVENT_SKIPOWNPROCESS`. Because our own windows are skipped, a
//!   500 ms poll of `GetForegroundWindow` also catches changes to and from
//!   the settings window (and anything a hook missed).
//! * Hook functions can be reentered when they call something that checks
//!   the message queue; state lives in a thread-local `RefCell` and a
//!   reentrant call is deferred to the message loop instead of panicking.
//! * WTSRegisterSessionNotification on the message-only window ("These
//!   notifications are sent only to the windows that have registered"),
//!   retried while Remote Desktop Services is not ready
//!   (`RPC_S_INVALID_BINDING`), unregistered before the window is destroyed.
//! * UI Automation runs on its own thread ([`super::uia`]) and posts its
//!   verdicts to this window, so every password decision is made here, on
//!   one thread, by the fail-closed [`PasswordGate`].

use std::cell::{Cell, RefCell};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle, Thread};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetMessageW, GetWindowThreadProcessId, KillTimer, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetTimer, TranslateMessage, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND,
    HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_DESTROY, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSEXW,
    WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

use super::super::{ContextEvent, ForegroundInfo, PasswordGate, PlatformError, Verdict};
use super::process::{self, ProcessCache};
use super::uia::UiaThread;
use super::{layout, now_ms, CURRENT_HKL, FOREGROUND_BLIND, SESSION_LOCKED};

const CLASS_NAME: PCWSTR = w!("IDEAKeystageWatcher");

/// Messages to the watcher window.
const MSG_STOP: u32 = WM_APP + 0x10;
const MSG_REFRESH: u32 = WM_APP + 0x11;
const MSG_UIA_VERDICT: u32 = WM_APP + 0x12;

const TIMER_POLL: usize = 1;
const POLL_MS: u32 = 500;
/// Retry session registration every 2 s for up to 2 minutes.
const WTS_RETRY_TICKS: u32 = 4;
const WTS_MAX_ATTEMPTS: u32 = 60;
/// Close handles of exited processes every 10 s.
const SWEEP_TICKS: u32 = 20;

const START_TIMEOUT: Duration = Duration::from_secs(5);

/// One watcher at a time.
static RUNNING: AtomicBool = AtomicBool::new(false);

thread_local! {
    static STATE: RefCell<Option<WatcherState>> = const { RefCell::new(None) };
    /// A WinEvent arrived while the state was busy; re-evaluate from the loop.
    static PENDING: Cell<bool> = const { Cell::new(false) };
    /// This thread's window, for posting to itself.
    static WINDOW: Cell<isize> = const { Cell::new(0) };
}

/// Status flags the handle reads from other threads.
#[derive(Debug, Default)]
struct Status {
    running: AtomicBool,
    /// Shared with the UI Automation thread, which sets it while subscribed.
    uia: Arc<AtomicBool>,
    session_notifications: AtomicBool,
}

struct WatcherState {
    hwnd: HWND,
    tx: Sender<ContextEvent>,
    consumer: Thread,
    status: Arc<Status>,
    win_hooks: Vec<HWINEVENTHOOK>,
    timer: bool,
    wts_attempts: u32,
    ticks: u32,
    fg_hwnd: isize,
    fg_tid: u32,
    fg: Option<ForegroundInfo>,
    hkl: isize,
    gate: PasswordGate,
    procs: ProcessCache,
    uia: Option<UiaThread>,
}

impl WatcherState {
    fn send(&self, ev: ContextEvent) {
        if self.tx.send(ev).is_ok() {
            self.consumer.unpark();
        }
    }

    /// Re-reads everything. `force` re-sends the foreground even if it did
    /// not change.
    fn refresh(&mut self, force: bool) {
        // SAFETY: no preconditions.
        let hwnd = unsafe { GetForegroundWindow() };
        self.on_foreground(hwnd, force);
        self.check_win32_password();
    }

    fn on_foreground(&mut self, hwnd: HWND, force: bool) {
        let raw = hwnd.0 as isize;
        if !force && raw == self.fg_hwnd {
            return;
        }
        self.fg_hwnd = raw;
        self.fg_tid = if hwnd.0.is_null() {
            0
        } else {
            // SAFETY: plain call.
            unsafe { GetWindowThreadProcessId(hwnd, None) }
        };
        let info = process::foreground_info(hwnd, Some(&mut self.procs));
        FOREGROUND_BLIND.store(info.is_blind(), Ordering::Release);
        if force || self.fg.as_ref() != Some(&info) {
            self.fg = Some(info.clone());
            self.send(ContextEvent::Foreground(info));
        }
        if super::desktop_readable() {
            self.send(ContextEvent::Modifiers(
                process::modifier_snapshot(now_ms()),
            ));
        }
        self.check_layout(false);
    }

    /// Sends a new legend table when the foreground layout changed. With
    /// `initial`, falls back to this thread's layout when there is no
    /// foreground window, so a table is always sent at start.
    fn check_layout(&mut self, initial: bool) {
        let hkl = if self.fg_tid != 0 {
            layout::thread_layout(self.fg_tid)
        } else if initial {
            layout::thread_layout(0)
        } else {
            None
        };
        let Some(hkl) = hkl else {
            return;
        };
        let raw = hkl.0 as isize;
        if raw == self.hkl {
            return;
        }
        self.hkl = raw;
        CURRENT_HKL.store(raw, Ordering::Release);
        self.send(ContextEvent::Layout(Arc::new(layout::build_table(hkl))));
    }

    fn check_win32_password(&mut self) {
        if let Some(active) = self.gate.on_win32(process::win32_password_verdict()) {
            self.send(ContextEvent::Password { active });
        }
    }

    fn on_uia(&mut self, verdict: Verdict) {
        if let Some(active) = self.gate.on_uia(verdict) {
            self.send(ContextEvent::Password { active });
        }
    }

    fn on_session(&mut self, code: u32) {
        match code {
            WTS_SESSION_LOCK => {
                SESSION_LOCKED.store(true, Ordering::Release);
                self.send(ContextEvent::SessionLocked { t_ms: now_ms() });
            }
            WTS_SESSION_UNLOCK => {
                SESSION_LOCKED.store(false, Ordering::Release);
                self.send(ContextEvent::SessionUnlocked { t_ms: now_ms() });
                // Resync (rule 7): sends Modifiers when the desktop is readable.
                self.refresh(true);
            }
            _ => {}
        }
    }

    fn register_session_notifications(&mut self) {
        if self.status.session_notifications.load(Ordering::Acquire)
            || self.wts_attempts >= WTS_MAX_ATTEMPTS
        {
            return;
        }
        self.wts_attempts += 1;
        // SAFETY: our own window, alive on this thread.
        match unsafe { WTSRegisterSessionNotification(self.hwnd, NOTIFY_FOR_THIS_SESSION) } {
            Ok(()) => {
                self.status
                    .session_notifications
                    .store(true, Ordering::Release);
            }
            Err(e) if self.wts_attempts >= WTS_MAX_ATTEMPTS => {
                tracing::warn!(
                    "session lock notifications are unavailable: {}",
                    PlatformError::os("WTSRegisterSessionNotification", &e)
                );
            }
            Err(_) => {}
        }
    }

    fn on_timer(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        // SAFETY: no preconditions.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0 as isize != self.fg_hwnd {
            self.on_foreground(hwnd, false);
            self.check_win32_password();
        } else {
            self.check_layout(false);
        }
        if self.ticks.is_multiple_of(WTS_RETRY_TICKS) {
            self.register_session_notifications();
        }
        if self.ticks.is_multiple_of(SWEEP_TICKS) {
            self.procs.sweep();
        }
    }

    /// Undoes everything, on this thread, before the window is destroyed.
    fn shutdown(&mut self) {
        if self.timer {
            // SAFETY: our own window and timer.
            let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_POLL) };
            self.timer = false;
        }
        for h in self.win_hooks.drain(..) {
            // SAFETY: hooks set on this thread.
            let _ = unsafe { UnhookWinEvent(h) };
        }
        if self
            .status
            .session_notifications
            .swap(false, Ordering::AcqRel)
        {
            // SAFETY: "For every call to this function, there must be a
            // corresponding call to WTSUnRegisterSessionNotification."
            let _ = unsafe { WTSUnRegisterSessionNotification(self.hwnd) };
        }
        if let Some(mut u) = self.uia.take() {
            u.stop();
        }
        FOREGROUND_BLIND.store(false, Ordering::Release);
        SESSION_LOCKED.store(false, Ordering::Release);
    }
}

/// Runs `f` on the watcher state. Returns false when the state is busy (a
/// reentrant call); the caller then defers the work.
fn with_state(f: impl FnOnce(&mut WatcherState)) -> bool {
    STATE.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => {
            if let Some(s) = guard.as_mut() {
                f(s);
            }
            true
        }
        Err(_) => false,
    })
}

fn post_to_self(message: u32, wparam: WPARAM, lparam: LPARAM) {
    let raw = WINDOW.with(Cell::get);
    if raw != 0 {
        // SAFETY: our own window.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(raw as *mut std::ffi::c_void)),
                message,
                wparam,
                lparam,
            )
        };
    }
}

fn defer_refresh() {
    if !PENDING.with(|p| p.replace(true)) {
        post_to_self(MSG_REFRESH, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let handled = match event {
            EVENT_SYSTEM_FOREGROUND => with_state(|s| {
                s.on_foreground(hwnd, false);
                s.check_win32_password();
            }),
            EVENT_OBJECT_FOCUS => with_state(WatcherState::check_win32_password),
            _ => true,
        };
        if !handled {
            defer_refresh();
        }
    }));
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = catch_unwind(AssertUnwindSafe(|| {
        handle_message(hwnd, msg, wparam, lparam)
    }));
    match handled {
        Ok(Some(r)) => r,
        // SAFETY: default processing for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn handle_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_WTSSESSION_CHANGE => {
            if !with_state(|s| s.on_session(wparam.0 as u32)) {
                // Busy: handle it again from the loop rather than lose it.
                post_to_self(msg, wparam, lparam);
            }
            Some(LRESULT(0))
        }
        MSG_UIA_VERDICT => {
            let verdict = Verdict::from_param(wparam.0);
            if !with_state(|s| s.on_uia(verdict)) {
                post_to_self(msg, wparam, lparam);
            }
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == TIMER_POLL => {
            with_state(WatcherState::on_timer);
            Some(LRESULT(0))
        }
        MSG_REFRESH => {
            PENDING.with(|p| p.set(false));
            if !with_state(|s| s.refresh(true)) {
                defer_refresh();
            }
            Some(LRESULT(0))
        }
        MSG_STOP => {
            with_state(WatcherState::shutdown);
            // SAFETY: our own window, on its thread.
            if unsafe { DestroyWindow(hwnd) }.is_err() {
                // SAFETY: no preconditions.
                unsafe { PostQuitMessage(0) };
            }
            Some(LRESULT(0))
        }
        WM_DESTROY => {
            // SAFETY: no preconditions.
            unsafe { PostQuitMessage(0) };
            Some(LRESULT(0))
        }
        _ => None,
    }
}

fn register_class(hinstance: HINSTANCE) -> Result<(), PlatformError> {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    let ok = *REGISTERED.get_or_init(|| {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        // SAFETY: a fully initialized class description.
        let atom = unsafe { RegisterClassExW(&class) };
        atom != 0
    });
    if ok {
        Ok(())
    } else {
        Err(PlatformError::last_os_error("RegisterClassExW"))
    }
}

fn create_window() -> Result<HWND, PlatformError> {
    // SAFETY: plain calls; the window belongs to this thread.
    unsafe {
        let module =
            GetModuleHandleW(None).map_err(|e| PlatformError::os("GetModuleHandleW", &e))?;
        let hinstance = HINSTANCE::from(module);
        register_class(hinstance)?;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            w!("IDEA Keystage watcher"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        )
        .map_err(|e| PlatformError::os("CreateWindowExW(HWND_MESSAGE)", &e))
    }
}

fn set_win_event_hook(event: u32) -> Option<HWINEVENTHOOK> {
    // SAFETY: out-of-context hook delivered to this thread's message loop.
    let h = unsafe {
        SetWinEventHook(
            event,
            event,
            None,
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    (!h.0.is_null()).then_some(h)
}

fn watcher_thread(
    tx: Sender<ContextEvent>,
    consumer: Thread,
    status: Arc<Status>,
    ready: SyncSender<Result<isize, PlatformError>>,
) {
    let hwnd = match create_window() {
        Ok(h) => h,
        Err(e) => {
            let _ = ready.send(Err(e));
            RUNNING.store(false, Ordering::Release);
            return;
        }
    };
    WINDOW.with(|w| w.set(hwnd.0 as isize));

    let mut win_hooks = Vec::new();
    for event in [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_FOCUS] {
        match set_win_event_hook(event) {
            Some(h) => win_hooks.push(h),
            None => tracing::warn!("a WinEvent hook could not be set; polling covers it"),
        }
    }
    // SAFETY: our own window.
    let timer = unsafe { SetTimer(Some(hwnd), TIMER_POLL, POLL_MS, None) } != 0;
    if !timer {
        tracing::warn!("the watcher poll timer could not be set");
    }
    let uia = match UiaThread::spawn(hwnd.0 as isize, MSG_UIA_VERDICT, status.uia.clone()) {
        Ok(u) => Some(u),
        Err(e) => {
            tracing::warn!("UI Automation password detection is unavailable: {e}");
            None
        }
    };
    let mut state = WatcherState {
        hwnd,
        tx,
        consumer,
        status: status.clone(),
        win_hooks,
        timer,
        wts_attempts: 0,
        ticks: 0,
        fg_hwnd: 0,
        fg_tid: 0,
        fg: None,
        hkl: 0,
        gate: PasswordGate::new(),
        procs: ProcessCache::new(),
        uia,
    };
    state.register_session_notifications();
    // The current state, so the app starts with a full picture.
    state.refresh(true);
    state.check_layout(true);
    state.send(ContextEvent::Password {
        active: state.gate.active(),
    });
    STATE.with(|cell| *cell.borrow_mut() = Some(state));
    status.running.store(true, Ordering::Release);

    if ready.send(Ok(hwnd.0 as isize)).is_err() {
        // `start_watcher` gave up waiting: clean up and leave.
        post_to_self(MSG_STOP, WPARAM(0), LPARAM(0));
    } else {
        tracing::info!("watcher started");
    }

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid MSG.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        match r.0 {
            0 => break,
            -1 => {
                tracing::error!("the watcher message loop failed");
                with_state(WatcherState::shutdown);
                // SAFETY: our own window, on its thread.
                let _ = unsafe { DestroyWindow(hwnd) };
                break;
            }
            _ => {}
        }
        // SAFETY: `msg` came from GetMessageW.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(mut s) = guard.take() {
                s.shutdown();
            }
        }
    });
    WINDOW.with(|w| w.set(0));
    status.running.store(false, Ordering::Release);
    tracing::info!("watcher stopped");
    RUNNING.store(false, Ordering::Release);
}

struct Inner {
    hwnd: isize,
    status: Arc<Status>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl Inner {
    fn post(&self, message: u32) -> Result<(), PlatformError> {
        // SAFETY: posting to the watcher window of this process.
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut std::ffi::c_void)),
                message,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .map_err(|e| PlatformError::os("PostMessageW", &e))
    }

    fn shutdown(&self) {
        let Some(join) = self.join.lock().take() else {
            return;
        };
        if self.post(MSG_STOP).is_ok() || join.is_finished() {
            let _ = join.join();
        } else {
            tracing::warn!("the watcher could not be told to stop");
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Controls the watcher. Clone it freely; the watcher stops when
/// [`WatcherHandle::shutdown`] is called or the last clone is dropped.
///
/// Threading: every method may be called from any thread.
#[derive(Clone)]
pub struct WatcherHandle {
    inner: Arc<Inner>,
}

impl WatcherHandle {
    /// Re-reads the foreground, layout, modifiers and Win32 password check
    /// now. Call it when one of the app's own windows gains or loses focus
    /// (WinEvents from our own process are skipped; the poll would catch it
    /// within half a second).
    pub fn refresh(&self) -> Result<(), PlatformError> {
        if !self.is_running() {
            return Err(PlatformError::NotRunning("watcher"));
        }
        self.inner.post(MSG_REFRESH)
    }

    pub fn is_running(&self) -> bool {
        self.inner.status.running.load(Ordering::Acquire)
    }

    /// True while UI Automation password detection is active. When false,
    /// only classic Edit password fields are detected.
    pub fn uia_active(&self) -> bool {
        self.inner.status.uia.load(Ordering::Acquire)
    }

    /// True once session lock and unlock notifications are registered.
    pub fn session_notifications(&self) -> bool {
        self.inner
            .status
            .session_notifications
            .load(Ordering::Acquire)
    }

    /// Stops the watcher and its UI Automation thread. Blocks until they
    /// have ended. Idempotent.
    pub fn shutdown(&self) {
        self.inner.shutdown();
    }
}

/// See [`crate::platform::start_watcher`].
pub fn start_watcher(
    consumer: Thread,
) -> Result<(WatcherHandle, Receiver<ContextEvent>), PlatformError> {
    if RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(PlatformError::AlreadyRunning("watcher"));
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    let (ready_tx, ready_rx) = sync_channel(1);
    let status = Arc::new(Status::default());
    let thread_status = status.clone();
    let join = thread::Builder::new()
        .name("keystage-watcher".into())
        .spawn(move || watcher_thread(tx, consumer, thread_status, ready_tx))
        .map_err(|source| {
            RUNNING.store(false, Ordering::Release);
            PlatformError::Spawn {
                name: "watcher",
                source,
            }
        })?;
    let hwnd = match ready_rx.recv_timeout(START_TIMEOUT) {
        Ok(Ok(hwnd)) => hwnd,
        Ok(Err(e)) => {
            let _ = join.join();
            return Err(e);
        }
        // The thread stops itself once it finds nobody waiting.
        Err(RecvTimeoutError::Timeout) => return Err(PlatformError::StartTimeout("watcher")),
        Err(RecvTimeoutError::Disconnected) => {
            let _ = join.join();
            RUNNING.store(false, Ordering::Release);
            return Err(PlatformError::ThreadExited("watcher"));
        }
    };
    let inner = Arc::new(Inner {
        hwnd,
        status,
        join: Mutex::new(Some(join)),
    });
    Ok((WatcherHandle { inner }, rx))
}
