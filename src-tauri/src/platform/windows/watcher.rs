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
//! * Password hold (`platform::hold`; ARCHITECTURE.md section 8, rule 3):
//!   every foreground change and every `EVENT_OBJECT_FOCUS` for a new object
//!   starts a new UI Automation request and a new `Hold` epoch, published
//!   to [`PASSWORD_HOLD`] before anything else is done, so the keys pressed
//!   from then on are held by the input receiver. The answer to that
//!   request releases them ("not a password") or drops them; so do a newer
//!   focus change, an unreadable element, and no answer within
//!   [`PENDING_TIMEOUT_MS`] (then `Password { active: true }` is sent until
//!   the answer comes). At start the hold begins before the first
//!   `Foreground` event is sent. `ContextEvent::Password` follows
//!   [`PasswordGate::app_suppressed`] and is always sent *before* the hold
//!   word changes: on the way out of a password field so the first keys
//!   handed out are not suppressed by a stale state, on the way in so the
//!   processor is suppressed before the modifier and Caps Lock records of a
//!   dropped epoch reach it (`Outcome::Drop`). A hold that ends while the
//!   app is not suppressing is `Outcome::Discard`: nothing of it passes.
//! * Every epoch started by a focus or foreground change is marked as a
//!   focus epoch, also when UI Automation does not run (then an `Open` or
//!   `Drop` epoch is started after the `Foreground` event and the Win32
//!   check). The input receiver decides the keys typed right after a Tab or
//!   a click, which the hook stamped before this thread heard of the focus
//!   change, by the first focus epoch after them (`platform::hold`). This
//!   thread runs at `THREAD_PRIORITY_HIGHEST` so it hears of focus changes
//!   soon on a loaded machine; it only waits for messages.
//! * On every foreground window change (also between two windows of one
//!   process) the overlay is put back on top of the topmost group without
//!   waiting for the main thread, and a modifier resync is requested from
//!   the input receiver, which reads `GetAsyncKeyState` after the hook
//!   records already queued. A snapshot read here and sent on the context
//!   channel would not be ordered with the hook stream ("the callback
//!   function is called before the asynchronous state of the key is
//!   updated", LowLevelKeyboardProc) and could re-press a modifier released
//!   a moment before. `ContextEvent::Modifiers` is sent only at start and on
//!   unlock, as documented.
//! * The 500 ms poll also compares a cheap monitor signature (rectangles,
//!   DPI, primary) so resolution, arrangement and scaling changes are
//!   noticed: the message-only window gets no `WM_DISPLAYCHANGE` ("does not
//!   receive broadcast messages", Window Features).

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
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
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

use keystage_core::ipc::Monitor;

use super::super::display::DisplayChangeWatch;
use super::super::hold::{HoldMode, Outcome};
use super::super::{ContextEvent, ForegroundInfo, GateStep, PasswordGate, PlatformError, Verdict};
use super::process::{self, ProcessCache};
use super::uia::{self, UiaThread};
use super::{
    hooks, layout, monitors, now_ms, window_style, CURRENT_HKL, FOREGROUND_BLIND, PASSWORD_HOLD,
    SESSION_LOCKED,
};

const CLASS_NAME: PCWSTR = w!("IDEAKeystageWatcher");

/// Messages to the watcher window.
const MSG_STOP: u32 = WM_APP + 0x10;
const MSG_REFRESH: u32 = WM_APP + 0x11;
const MSG_UIA_ANSWER: u32 = WM_APP + 0x12;
const MSG_UIA_EVENT: u32 = WM_APP + 0x13;
const MSG_UIA_UNAVAILABLE: u32 = WM_APP + 0x14;

const TIMER_POLL: usize = 1;
const POLL_MS: u32 = 500;
/// Fires when UI Automation has not answered the awaited request in time.
const TIMER_PENDING: usize = 2;
/// Fires to ask UI Automation again after an `Unknown` answer.
const TIMER_RETRY: usize = 3;
/// How long input is held for a focus change before the focus counts as a
/// password field until UI Automation answers.
const PENDING_TIMEOUT_MS: u32 = 400;
/// The same while UI Automation is still starting (creating the client and
/// subscribing can take a while at logon).
const START_PENDING_TIMEOUT_MS: u32 = 3_000;
/// Delay before asking again after an unreadable element.
const RETRY_MS: u32 = 250;
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

/// How a foreground read resyncs modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModifierSync {
    /// Ask the input receiver, which reads after the queued hook records
    /// (every foreground change).
    Ordered,
    /// Send `ContextEvent::Modifiers` (the documented start and unlock
    /// events).
    Event,
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
    /// The hold mode last published to [`PASSWORD_HOLD`].
    published: HoldMode,
    /// The focus moved but no focus epoch was published for it (UI
    /// Automation is not running): [`WatcherState::settle_focus`] publishes
    /// one, unless a transition takes the mark first.
    focus_due: bool,
    /// The last `Password { active }` sent, once the start event was sent.
    sent_active: Option<bool>,
    /// The request the pending timer runs for.
    pending_timer: Option<u32>,
    /// The object of the last `EVENT_OBJECT_FOCUS` (window, object, child),
    /// forgotten on every foreground change.
    last_focus: Option<(isize, i32, i32)>,
    procs: ProcessCache,
    uia: Option<UiaThread>,
    displays: DisplayChangeWatch<Vec<Monitor>>,
}

impl WatcherState {
    fn send(&self, ev: ContextEvent) {
        if self.tx.send(ev).is_ok() {
            self.consumer.unpark();
        }
    }

    /// Re-reads everything. `force` re-sends the foreground even if it did
    /// not change.
    fn refresh(&mut self, force: bool, mods: ModifierSync) {
        // SAFETY: no preconditions.
        let hwnd = unsafe { GetForegroundWindow() };
        self.on_foreground(hwnd, force, mods);
        self.settle_focus();
    }

    fn on_foreground(&mut self, hwnd: HWND, force: bool, mods: ModifierSync) {
        let raw = hwnd.0 as isize;
        if !force && raw == self.fg_hwnd {
            return;
        }
        self.fg_hwnd = raw;
        // Hold input first: the focus is somewhere new. The next focus event
        // always counts as new, whatever object it names.
        self.last_focus = None;
        self.focus_changed();
        // Every foreground window change, also between two windows of one
        // process (which `Foreground` does not report): a topmost window
        // that just came forward must not cover the overlay (ARCHITECTURE.md
        // section 6).
        window_style::reassert_overlay_topmost_async();
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
            match mods {
                ModifierSync::Ordered => hooks::request_resync(),
                ModifierSync::Event => self.send_modifiers(),
            }
        }
        self.check_layout(false);
    }

    /// Sends `ContextEvent::Modifiers` when the desktop is readable.
    fn send_modifiers(&self) {
        if let Some(snapshot) = super::read_modifiers() {
            self.send(ContextEvent::Modifiers(snapshot));
        }
    }

    /// Notices a settled change of the monitor configuration.
    fn check_displays(&mut self) {
        if let Some(signature) = monitors::layout_signature() {
            if self.displays.observe(signature) {
                // Platform side of the display-change signal; the event that
                // carries it to the app needs a ContextEvent variant
                // (.work/notes/fix-requests.md, WIN-3).
                tracing::info!("display configuration changed");
            }
        }
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
        let step = self.gate.on_win32(process::win32_password_verdict());
        self.apply(step);
    }

    /// After a foreground or focus event: the Win32 check, then the focus
    /// epoch a focus change without UI Automation still owes the input
    /// receiver. It comes after the `Foreground` event and the Win32 verdict
    /// so the keys it lets out are handled under both.
    fn settle_focus(&mut self) {
        self.check_win32_password();
        if std::mem::take(&mut self.focus_due) && self.published != HoldMode::Hold {
            // The outcome is unused: the epoch being left is not a `Hold`.
            PASSWORD_HOLD.publish(self.published, Outcome::Discard, true);
            self.consumer.unpark();
        }
    }

    /// The focus moved (or may have): a new UI Automation request, input
    /// held until it is answered (a focus epoch either way).
    fn focus_changed(&mut self) {
        let step = self.gate.on_focus_changed();
        self.apply(step);
        if !step.new_focus {
            self.focus_due = true;
        }
    }

    /// `EVENT_OBJECT_FOCUS`. Repeats for the object that already has the
    /// focus are not a focus change (some apps raise it again and again).
    fn on_focus_event(&mut self, hwnd: HWND, object: i32, child: i32) {
        let key = (hwnd.0 as isize, object, child);
        if self.last_focus != Some(key) {
            self.last_focus = Some(key);
            self.focus_changed();
        }
        self.settle_focus();
    }

    fn on_uia_answer(&mut self, request: u32, verdict: Verdict) {
        let step = self.gate.on_uia_answer(request, verdict);
        self.apply(step);
    }

    fn on_uia_event(&mut self, verdict: Verdict) {
        let step = self.gate.on_uia_event(verdict);
        self.apply(step);
    }

    fn on_uia_unavailable(&mut self) {
        if let Some(mut u) = self.uia.take() {
            u.stop();
        }
        let step = self.gate.on_uia_unavailable();
        self.apply(step);
    }

    /// Carries out a gate step: the UI Automation request, the hold, the
    /// `Password` event and the timers.
    fn apply(&mut self, step: GateStep) {
        if let (Some(id), Some(u)) = (step.request, self.uia.as_ref()) {
            u.classify(id);
        }
        let active = self
            .gate
            .app_suppressed(self.sent_active.unwrap_or_else(|| self.gate.active()));
        // Tell the app first, then change the hold. Leaving a password
        // field: the first keys handed out afterwards must not be suppressed
        // by a stale state. Entering one (or a focus that could not be
        // classified): the processor must be suppressed before the modifier
        // and Caps Lock records of the dropped input reach it. Before the
        // start event only a suppression is sent early.
        if self.sent_active != Some(active) && (self.sent_active.is_some() || active) {
            self.sent_active = Some(active);
            self.send(ContextEvent::Password { active });
        }
        if let Some((mode, outcome)) = self.gate.hold_transition(self.published, &step, active) {
            let focus = step.new_focus || std::mem::take(&mut self.focus_due);
            PASSWORD_HOLD.publish(mode, outcome, focus);
            self.published = mode;
            // The receiver releases or drops what it holds on its next drain.
            self.consumer.unpark();
        }
        self.arm_timers(step.retry);
    }

    fn arm_timers(&mut self, retry: bool) {
        let awaiting = self.gate.awaiting();
        if awaiting != self.pending_timer {
            self.pending_timer = awaiting;
            if awaiting.is_some() {
                let ms = if self.status.uia.load(Ordering::Acquire) {
                    PENDING_TIMEOUT_MS
                } else {
                    START_PENDING_TIMEOUT_MS
                };
                // SAFETY: our own window; replaces a running timer of this id.
                if unsafe { SetTimer(Some(self.hwnd), TIMER_PENDING, ms, None) } == 0 {
                    // Without the timer the hold lasts until the answer
                    // (or until the receiver's bound): still closed.
                    self.pending_timer = None;
                }
            } else {
                // SAFETY: our own window and timer.
                let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_PENDING) };
            }
        }
        if retry {
            // SAFETY: our own window.
            let _ = unsafe { SetTimer(Some(self.hwnd), TIMER_RETRY, RETRY_MS, None) };
        }
    }

    fn on_pending_timeout(&mut self) {
        // SAFETY: our own window and timer (one shot).
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_PENDING) };
        if let Some(request) = self.pending_timer.take() {
            let step = self.gate.on_timeout(request);
            self.apply(step);
        }
    }

    fn on_retry(&mut self) {
        // SAFETY: our own window and timer (one shot).
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_RETRY) };
        let step = self.gate.on_retry();
        self.apply(step);
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
                self.refresh(true, ModifierSync::Event);
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
            self.on_foreground(hwnd, false, ModifierSync::Ordered);
            self.settle_focus();
        } else {
            self.check_layout(false);
        }
        self.check_displays();
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
        for timer in [TIMER_PENDING, TIMER_RETRY] {
            // SAFETY: our own window; harmless if the timer is not set.
            let _ = unsafe { KillTimer(Some(self.hwnd), timer) };
        }
        // Nobody resolves the hold any more: drop what is held and stop
        // holding (the app shell stops input with the watcher).
        if self.published != HoldMode::Open {
            PASSWORD_HOLD.publish(HoldMode::Open, Outcome::Discard, false);
            self.published = HoldMode::Open;
            self.consumer.unpark();
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
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let handled = match event {
            EVENT_SYSTEM_FOREGROUND => with_state(|s| {
                s.on_foreground(hwnd, false, ModifierSync::Ordered);
                s.settle_focus();
            }),
            EVENT_OBJECT_FOCUS => with_state(|s| s.on_focus_event(hwnd, id_object, id_child)),
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
        MSG_UIA_ANSWER => {
            let verdict = Verdict::from_param(wparam.0);
            let request = lparam.0 as u32;
            if !with_state(|s| s.on_uia_answer(request, verdict)) {
                post_to_self(msg, wparam, lparam);
            }
            Some(LRESULT(0))
        }
        MSG_UIA_EVENT => {
            let verdict = Verdict::from_param(wparam.0);
            if !with_state(|s| s.on_uia_event(verdict)) {
                post_to_self(msg, wparam, lparam);
            }
            Some(LRESULT(0))
        }
        MSG_UIA_UNAVAILABLE => {
            if !with_state(WatcherState::on_uia_unavailable) {
                post_to_self(msg, wparam, lparam);
            }
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == TIMER_POLL => {
            with_state(WatcherState::on_timer);
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == TIMER_PENDING => {
            if !with_state(WatcherState::on_pending_timeout) {
                post_to_self(msg, wparam, lparam);
            }
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == TIMER_RETRY => {
            if !with_state(WatcherState::on_retry) {
                post_to_self(msg, wparam, lparam);
            }
            Some(LRESULT(0))
        }
        MSG_REFRESH => {
            PENDING.with(|p| p.set(false));
            if !with_state(|s| s.refresh(true, ModifierSync::Ordered)) {
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
    // Hear of focus changes soon on a loaded machine: keys typed right
    // after a Tab or a click wait for them (`platform::hold`). The thread
    // only waits for messages, so it takes little time from others.
    // SAFETY: the pseudo handle of this thread.
    if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) }.is_err() {
        tracing::warn!("the watcher thread priority could not be raised");
    }
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
    let messages = uia::Messages {
        answer: MSG_UIA_ANSWER,
        event: MSG_UIA_EVENT,
        unavailable: MSG_UIA_UNAVAILABLE,
    };
    let uia = match UiaThread::spawn(hwnd.0 as isize, messages, status.uia.clone()) {
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
        published: PASSWORD_HOLD.mode(),
        focus_due: false,
        sent_active: None,
        pending_timer: None,
        last_focus: None,
        procs: ProcessCache::new(),
        uia,
        displays: DisplayChangeWatch::new(),
    };
    state.register_session_notifications();
    // Hold input until UI Automation has classified the focus, before the
    // app hears about the foreground (fail closed from the first event).
    let first = if state.uia.is_some() {
        state.gate.on_uia_started()
    } else {
        GateStep::default()
    };
    state.apply(first);
    // The current state, so the app starts with a full picture.
    state.refresh(true, ModifierSync::Event);
    state.check_layout(true);
    state.check_displays();
    // Once at start (unless a suppression was sent already).
    let active = state
        .gate
        .app_suppressed(state.sent_active.unwrap_or(false));
    if state.sent_active != Some(active) {
        state.sent_active = Some(active);
        state.send(ContextEvent::Password { active });
    }
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
