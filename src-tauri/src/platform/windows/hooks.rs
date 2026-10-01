//! Low-level keyboard and mouse hooks on a dedicated thread.
//!
//! Microsoft's constraints (LowLevelKeyboardProc and LowLevelMouseProc,
//! read 2026-10-01; `.work/research/windows.md` sections 1.1 to 1.9 and the
//! "Design constraints for the hook layer"):
//!
//! * "the thread that installed the hook must have a message loop": the
//!   `keystage-hooks` thread installs both hooks and runs `GetMessageW`.
//! * A hook that exceeds `LowLevelHooksTimeout` (at most 1000 ms) is
//!   "silently removed without being called". The procedures therefore only
//!   copy a [`RawEvent`] into the preallocated lock-free queue, unpark the
//!   consumer and call `CallNextHookEx`: no allocation, logging, locks or
//!   channel sends, and nothing that can panic across the FFI boundary. The
//!   only other work is one atomic load: the password hold word copied into
//!   the record ([`super::PASSWORD_HOLD`], `platform::hold`).
//! * Windows removes only the hook whose call timed out, and a call times
//!   out when the hook thread does not run for that long (a SolidWorks
//!   rebuild and an OBS encode can saturate every core). So the hook thread
//!   runs at `THREAD_PRIORITY_TIME_CRITICAL`, and its own 50 ms liveness
//!   timer measures how long it went without running: after a stall of
//!   150 ms or more (`convert::STALL_THRESHOLD_MS`) it reinstalls both
//!   hooks at once. That also covers a lost keyboard hook while the mouse
//!   hook still delivers, which no `GetLastInputInfo` comparison can see.
//! * A watchdog thread compares `GetLastInputInfo` with the last record a
//!   hook delivered and asks the hook thread to reinstall when Windows saw
//!   input the hooks missed.
//! * Stop: post a private `WM_APP` message; the hook thread unhooks and
//!   calls `PostQuitMessage` itself ("Do not post the WM_QUIT message using
//!   the PostMessage function", WM_QUIT). Posting waits for the thread's
//!   queue, created with `PeekMessageW(PM_NOREMOVE)` before it reports
//!   ready.
//! * Key state is never read inside a procedure ("the asynchronous state of
//!   the key cannot be determined by calling GetAsyncKeyState from within
//!   the callback function").

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

use keystage_core::{Mod, Side};
use parking_lot::Mutex;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KillTimer, PeekMessageW, PostQuitMessage,
    PostThreadMessageW, SetCoalescableTimer, SetTimer, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
    TIMERV_NO_COALESCING, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_TIMER, WM_USER,
};

use super::super::convert::{
    self, RawEvent, StallProbe, STALL_PROBE_MS, STALL_THRESHOLD_MS, WATCHDOG_THRESHOLD_MS,
};
use super::super::input::{InputReceiver, InputShared};
use super::super::PlatformError;
use super::{layout, os, process};

// The pure conversion module spells the documented constants out; make sure
// they match the Windows headers as the `windows` crate states them.
const _: () = {
    use windows::Win32::UI::WindowsAndMessaging as wm;
    assert!(convert::msg::WM_KEYDOWN == wm::WM_KEYDOWN);
    assert!(convert::msg::WM_KEYUP == wm::WM_KEYUP);
    assert!(convert::msg::WM_SYSKEYDOWN == wm::WM_SYSKEYDOWN);
    assert!(convert::msg::WM_SYSKEYUP == wm::WM_SYSKEYUP);
    assert!(convert::msg::WM_MOUSEMOVE == wm::WM_MOUSEMOVE);
    assert!(convert::msg::WM_LBUTTONDOWN == wm::WM_LBUTTONDOWN);
    assert!(convert::msg::WM_LBUTTONUP == wm::WM_LBUTTONUP);
    assert!(convert::msg::WM_LBUTTONDBLCLK == wm::WM_LBUTTONDBLCLK);
    assert!(convert::msg::WM_RBUTTONDOWN == wm::WM_RBUTTONDOWN);
    assert!(convert::msg::WM_RBUTTONUP == wm::WM_RBUTTONUP);
    assert!(convert::msg::WM_RBUTTONDBLCLK == wm::WM_RBUTTONDBLCLK);
    assert!(convert::msg::WM_MBUTTONDOWN == wm::WM_MBUTTONDOWN);
    assert!(convert::msg::WM_MBUTTONUP == wm::WM_MBUTTONUP);
    assert!(convert::msg::WM_MBUTTONDBLCLK == wm::WM_MBUTTONDBLCLK);
    assert!(convert::msg::WM_MOUSEWHEEL == wm::WM_MOUSEWHEEL);
    assert!(convert::msg::WM_XBUTTONDOWN == wm::WM_XBUTTONDOWN);
    assert!(convert::msg::WM_XBUTTONUP == wm::WM_XBUTTONUP);
    assert!(convert::msg::WM_XBUTTONDBLCLK == wm::WM_XBUTTONDBLCLK);
    assert!(convert::msg::WM_MOUSEHWHEEL == wm::WM_MOUSEHWHEEL);
    assert!(convert::flags::LLKHF_EXTENDED == wm::LLKHF_EXTENDED.0);
    assert!(convert::flags::LLKHF_LOWER_IL_INJECTED == wm::LLKHF_LOWER_IL_INJECTED.0);
    assert!(convert::flags::LLKHF_INJECTED == wm::LLKHF_INJECTED.0);
    assert!(convert::flags::LLKHF_ALTDOWN == wm::LLKHF_ALTDOWN.0);
    assert!(convert::flags::LLKHF_UP == wm::LLKHF_UP.0);
    assert!(convert::flags::LLMHF_INJECTED == wm::LLMHF_INJECTED);
    assert!(convert::flags::LLMHF_LOWER_IL_INJECTED == wm::LLMHF_LOWER_IL_INJECTED);
    assert!(convert::XBUTTON1 == wm::XBUTTON1);
    assert!(convert::XBUTTON2 == wm::XBUTTON2);
    assert!(convert::VK_PACKET == windows::Win32::UI::Input::KeyboardAndMouse::VK_PACKET.0);
};

/// Thread messages the hook thread understands.
const MSG_PAUSE: u32 = WM_APP + 1;
const MSG_RESUME: u32 = WM_APP + 2;
const MSG_REINSTALL: u32 = WM_APP + 3;
const MSG_STOP: u32 = WM_APP + 4;
/// Tests only: block the hook thread for `wParam` ms, as a starved thread
/// would be.
#[cfg(test)]
const MSG_TEST_STALL: u32 = WM_APP + 5;

/// How long `start_input` waits for the hooks.
const START_TIMEOUT: Duration = Duration::from_secs(5);
/// Watchdog period.
const WATCHDOG_PERIOD: Duration = Duration::from_millis(1_000);
/// Minimum time between two watchdog reinstalls.
const REINSTALL_GAP: Duration = Duration::from_secs(10);
/// How long the hooks may be missing while not paused (the moment between
/// `resume()` and the hook thread reinstalling them) before it counts.
const MISSING_GRACE: Duration = Duration::from_secs(2);

/// State the hook procedures read. Set once, before the first install, and
/// reused if input is restarted.
static SHARED: OnceLock<Arc<InputShared>> = OnceLock::new();
/// The consumer the procedures unpark, stored before the hooks install.
static CONSUMER: OnceLock<Thread> = OnceLock::new();
/// One input hook at a time.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Reinstalls the hook thread made after a stall. The watchdog thread logs
/// them: the hook thread itself never waits on a log writer.
static STALL_REINSTALLS: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Hook procedures: copy, wake, chain. Nothing else.
// ---------------------------------------------------------------------------

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // "If nCode is less than zero, the hook procedure must pass the message
    // to the CallNextHookEx function without further processing."
    if code == HC_ACTION as i32 {
        let _ = catch_unwind(AssertUnwindSafe(|| record_keyboard(wparam, lparam)));
    }
    // SAFETY: forwarding the arguments we were given; hhk is ignored.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let _ = catch_unwind(AssertUnwindSafe(|| record_mouse(wparam, lparam)));
    }
    // SAFETY: as above.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[inline(always)]
fn record_keyboard(wparam: WPARAM, lparam: LPARAM) {
    let Some(shared) = SHARED.get() else {
        return;
    };
    if shared.paused.load(Ordering::Relaxed) {
        return;
    }
    let ptr = lparam.0 as *const KBDLLHOOKSTRUCT;
    if ptr.is_null() {
        return;
    }
    // The password state the watcher knows as this key is pressed.
    let hold = super::PASSWORD_HOLD.stamp();
    // SAFETY: for HC_ACTION, lParam points to a KBDLLHOOKSTRUCT that is
    // valid for the duration of this call.
    let k = unsafe { ptr.read() };
    shared.record(
        RawEvent::key(wparam.0 as u32, k.vkCode, k.scanCode, k.flags.0, k.time).with_hold(hold),
    );
    wake();
}

#[inline(always)]
fn record_mouse(wparam: WPARAM, lparam: LPARAM) {
    let Some(shared) = SHARED.get() else {
        return;
    };
    if shared.paused.load(Ordering::Relaxed) {
        return;
    }
    let ptr = lparam.0 as *const MSLLHOOKSTRUCT;
    if ptr.is_null() {
        return;
    }
    let hold = super::PASSWORD_HOLD.stamp();
    // SAFETY: for HC_ACTION, lParam points to an MSLLHOOKSTRUCT that is
    // valid for the duration of this call.
    let m = unsafe { ptr.read() };
    shared.record(
        RawEvent::mouse(
            wparam.0 as u32,
            m.pt.x,
            m.pt.y,
            m.mouseData,
            m.flags,
            m.time,
        )
        .with_hold(hold),
    );
    wake();
}

#[inline(always)]
fn wake() {
    if let Some(t) = CONSUMER.get() {
        t.unpark();
    }
}

/// Asks the input receiver for a modifier resync and wakes the consumer.
/// The receiver reads `GetAsyncKeyState` inside `drain_into`, after every
/// hook record queued so far, so the snapshot is ordered with the hook
/// stream (a snapshot sent on another channel could be applied after a
/// later key-up and re-press the key). No-op before input starts.
///
/// Threading: any thread except a hook procedure.
pub(crate) fn request_resync() {
    if let Some(shared) = SHARED.get() {
        shared.request_resync();
        wake();
    }
}

/// `held`, a `GetAsyncKeyState` read made at `now`, corrected by the hook
/// stream ([`InputShared::reconcile_held`]). Unchanged before input starts.
///
/// Threading: any thread except a hook procedure.
pub(crate) fn reconcile_held(held: Vec<(Mod, Side)>, now: u64) -> Vec<(Mod, Side)> {
    match SHARED.get() {
        Some(shared) => shared.reconcile_held(&held, now),
        None => held,
    }
}

// ---------------------------------------------------------------------------
// The hook thread
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Hooks {
    keyboard: Option<HHOOK>,
    mouse: Option<HHOOK>,
}

impl Hooks {
    /// Installs both hooks with the EXE's module handle ("An error may occur
    /// if the hMod parameter is NULL and the dwThreadId parameter is zero",
    /// SetWindowsHookExW). Installs nothing if either fails.
    fn install(&mut self, shared: &InputShared) -> Result<(), PlatformError> {
        if self.keyboard.is_some() && self.mouse.is_some() {
            return Ok(());
        }
        self.remove(shared);
        // SAFETY: plain calls; the procedures live for the whole process.
        unsafe {
            let module =
                GetModuleHandleW(None).map_err(|e| PlatformError::os("GetModuleHandleW", &e))?;
            let hmod = Some(HINSTANCE::from(module));
            let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0)
                .map_err(|e| PlatformError::os("SetWindowsHookExW(WH_KEYBOARD_LL)", &e))?;
            let mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0) {
                Ok(h) => h,
                Err(e) => {
                    let _ = UnhookWindowsHookEx(kb);
                    return Err(PlatformError::os("SetWindowsHookExW(WH_MOUSE_LL)", &e));
                }
            };
            self.keyboard = Some(kb);
            self.mouse = Some(mouse);
        }
        shared
            .last_hook_tick
            .store(os::tick_count(), Ordering::Relaxed);
        shared.installed.store(true, Ordering::Release);
        // Input may have been missed while the hooks were out: resync
        // modifiers on the next drain, and wake the consumer for it.
        shared.request_resync();
        wake();
        Ok(())
    }

    /// Removes both hooks and installs them again: Windows may have removed
    /// either one without saying so.
    fn reinstall(&mut self, shared: &InputShared) -> Result<(), PlatformError> {
        self.remove(shared);
        self.install(shared)
    }

    fn remove(&mut self, shared: &InputShared) {
        shared.installed.store(false, Ordering::Release);
        for h in [self.keyboard.take(), self.mouse.take()]
            .into_iter()
            .flatten()
        {
            // SAFETY: a handle this thread installed. "Before terminating, an
            // application must call the UnhookWindowsHookEx function."
            if unsafe { UnhookWindowsHookEx(h) }.is_err() {
                tracing::warn!("an input hook could not be removed");
            }
        }
    }
}

/// Starts this thread's liveness timer (a thread timer: `WM_TIMER` arrives
/// with a NULL `hwnd`). Coalescing is off so a healthy gap stays close to
/// the period. Returns the timer id, 0 when no timer could be set.
fn start_probe_timer() -> usize {
    // SAFETY: a thread timer without a callback, on this thread's queue.
    unsafe {
        let id = SetCoalescableTimer(None, 0, STALL_PROBE_MS, None, TIMERV_NO_COALESCING);
        if id != 0 {
            id
        } else {
            SetTimer(None, 0, STALL_PROBE_MS, None)
        }
    }
}

fn hook_thread(shared: Arc<InputShared>, ready: SyncSender<Result<u32, PlatformError>>) {
    // A hook call that waits longer than LowLevelHooksTimeout for this
    // thread loses that hook. The thread does microseconds of work per event
    // and sleeps otherwise, so keep it ahead of the normal-priority threads
    // that saturate the CPU during a rebuild while OBS encodes.
    // SAFETY: the pseudo handle of the calling thread.
    if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) }.is_err() {
        tracing::warn!("the input hook thread could not raise its priority");
    }
    let mut msg = MSG::default();
    // Create this thread's message queue before anyone posts to it
    // (PostThreadMessageW's documented handshake).
    // SAFETY: `msg` is a valid MSG.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
    }
    // SAFETY: no preconditions.
    let tid = unsafe { GetCurrentThreadId() };
    let mut hooks = Hooks::default();
    if let Err(e) = hooks.install(&shared) {
        let _ = ready.send(Err(e));
        RUNNING.store(false, Ordering::Release);
        return;
    }
    let probe_timer = start_probe_timer();
    if ready.send(Ok(tid)).is_err() {
        // `start_input` gave up waiting; do not leave hooks behind.
        hooks.remove(&shared);
        stop_probe_timer(probe_timer);
        RUNNING.store(false, Ordering::Release);
        return;
    }
    tracing::info!("input hooks installed");
    if probe_timer == 0 {
        tracing::warn!("the input hook liveness timer could not be set");
    }
    let mut probe = StallProbe::new();
    probe.tick(os::tick_count64(), STALL_THRESHOLD_MS);

    loop {
        // SAFETY: `msg` is a valid MSG; NULL hwnd also returns thread messages.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        match r.0 {
            0 => break, // WM_QUIT from our own PostQuitMessage
            -1 => {
                tracing::error!("the input hook message loop failed; hooks removed");
                break;
            }
            _ => {}
        }
        if msg.hwnd.0.is_null() {
            match msg.message {
                WM_TIMER if probe_timer != 0 && msg.wParam.0 == probe_timer => {
                    let stall = probe.tick(os::tick_count64(), STALL_THRESHOLD_MS);
                    if stall.is_some() && !shared.paused.load(Ordering::Acquire) {
                        // Any hook call made during the stall may have timed
                        // out, and Windows does not say which hook it
                        // removed.
                        // Nothing is logged here (a log writer may block on
                        // disk under exactly this load); the watchdog
                        // reports it, and retries a failed install.
                        shared.reinstalls.fetch_add(1, Ordering::Relaxed);
                        STALL_REINSTALLS.fetch_add(1, Ordering::Relaxed);
                        let _ = hooks.reinstall(&shared);
                    }
                }
                MSG_PAUSE => {
                    hooks.remove(&shared);
                    shared.clear();
                    tracing::info!("input hooks removed (paused)");
                }
                MSG_RESUME => {
                    if !shared.paused.load(Ordering::Acquire) {
                        match hooks.install(&shared) {
                            Ok(()) => tracing::info!("input hooks installed (resumed)"),
                            Err(e) => tracing::error!("input hooks could not be reinstalled: {e}"),
                        }
                    }
                }
                MSG_REINSTALL => {
                    if !shared.paused.load(Ordering::Acquire) {
                        if let Err(e) = hooks.reinstall(&shared) {
                            tracing::error!("input hooks could not be reinstalled: {e}");
                        }
                    }
                }
                MSG_STOP => {
                    hooks.remove(&shared);
                    // SAFETY: no preconditions.
                    unsafe { PostQuitMessage(0) };
                }
                #[cfg(test)]
                MSG_TEST_STALL => thread::sleep(Duration::from_millis(msg.wParam.0 as u64)),
                _ => {}
            }
            continue;
        }
        // This thread owns no windows; dispatch anything addressed to one
        // anyway, as a message loop should.
        // SAFETY: `msg` came from GetMessageW.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    hooks.remove(&shared);
    stop_probe_timer(probe_timer);
    tracing::info!("input hook thread stopped");
    RUNNING.store(false, Ordering::Release);
}

fn stop_probe_timer(id: usize) {
    if id != 0 {
        // SAFETY: a thread timer this thread set.
        let _ = unsafe { KillTimer(None, id) };
    }
}

// ---------------------------------------------------------------------------
// Watchdog
// ---------------------------------------------------------------------------

fn watchdog(shared: Arc<InputShared>, hook_tid: u32, stopping: Arc<AtomicBool>) {
    let mut last_request: Option<Instant> = None;
    let mut missing_since: Option<Instant> = None;
    let mut stalls_seen = STALL_REINSTALLS.load(Ordering::Relaxed);
    loop {
        thread::park_timeout(WATCHDOG_PERIOD);
        if stopping.load(Ordering::Acquire) {
            break;
        }
        let stalls = STALL_REINSTALLS.load(Ordering::Relaxed);
        if stalls != stalls_seen {
            stalls_seen = stalls;
            tracing::warn!("the input hook thread stalled; hooks reinstalled");
        }
        if shared.paused.load(Ordering::Acquire) || !super::desktop_readable() {
            missing_since = None;
            continue;
        }
        let installed = shared.installed.load(Ordering::Acquire);
        if installed {
            missing_since = None;
        }
        let needed = if installed {
            let mut lii = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            // SAFETY: `lii` is a valid LASTINPUTINFO with its size set.
            if !unsafe { GetLastInputInfo(&mut lii) }.as_bool() {
                continue;
            }
            convert::hook_looks_dead(
                lii.dwTime,
                shared.last_hook_tick.load(Ordering::Relaxed),
                WATCHDOG_THRESHOLD_MS,
            )
        } else {
            // Not installed while not paused: a resume or reinstall failed,
            // unless the hook thread simply has not caught up yet.
            let since = *missing_since.get_or_insert_with(Instant::now);
            since.elapsed() >= MISSING_GRACE
        };
        if !needed || last_request.is_some_and(|t| t.elapsed() < REINSTALL_GAP) {
            continue;
        }
        // SAFETY: posting a private thread message.
        if unsafe { PostThreadMessageW(hook_tid, MSG_REINSTALL, WPARAM(0), LPARAM(0)) }.is_ok() {
            shared.reinstalls.fetch_add(1, Ordering::Relaxed);
            last_request = Some(Instant::now());
            tracing::warn!("input hooks looked removed by Windows; reinstalling");
        }
    }
}

// ---------------------------------------------------------------------------
// Public handle
// ---------------------------------------------------------------------------

struct Inner {
    shared: Arc<InputShared>,
    tid: u32,
    hook_thread: Mutex<Option<JoinHandle<()>>>,
    watchdog: Mutex<Option<JoinHandle<()>>>,
    stopping: Arc<AtomicBool>,
}

impl Inner {
    fn post(&self, message: u32) -> Result<(), PlatformError> {
        self.post_with(message, 0)
    }

    fn post_with(&self, message: u32, wparam: usize) -> Result<(), PlatformError> {
        // SAFETY: posting a private thread message to our hook thread.
        unsafe { PostThreadMessageW(self.tid, message, WPARAM(wparam), LPARAM(0)) }
            .map_err(|e| PlatformError::os("PostThreadMessageW", &e))
    }

    fn shutdown(&self) {
        if self.stopping.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(w) = self.watchdog.lock().take() {
            w.thread().unpark();
            let _ = w.join();
        }
        if let Some(h) = self.hook_thread.lock().take() {
            if self.post(MSG_STOP).is_ok() || h.is_finished() {
                let _ = h.join();
            } else {
                // The thread is gone or unreachable; it removes the hooks on
                // its way out of the loop.
                tracing::warn!("the input hook thread could not be told to stop");
            }
        }
        self.shared.clear();
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Controls the input hooks. Clone it freely; the hooks stop when
/// [`InputHandle::shutdown`] is called or the last clone is dropped.
///
/// Threading: every method may be called from any thread.
#[derive(Clone)]
pub struct InputHandle {
    inner: Arc<Inner>,
}

impl InputHandle {
    /// Stops recording at once and removes both hooks (ARCHITECTURE.md
    /// section 8, rule 2). Anything still queued is discarded.
    pub fn pause(&self) -> Result<(), PlatformError> {
        self.inner.shared.paused.store(true, Ordering::Release);
        self.inner.shared.clear();
        self.inner.post(MSG_PAUSE)
    }

    /// Reinstalls both hooks. The receiver then adds a modifier resync on
    /// its next drain by itself.
    pub fn resume(&self) -> Result<(), PlatformError> {
        self.inner.shared.paused.store(false, Ordering::Release);
        self.inner.post(MSG_RESUME)
    }

    pub fn is_paused(&self) -> bool {
        self.inner.shared.paused.load(Ordering::Acquire)
    }

    /// Both hooks are installed right now.
    pub fn is_installed(&self) -> bool {
        self.inner.shared.installed.load(Ordering::Acquire)
    }

    /// Records lost because the queue was full (should stay 0).
    pub fn dropped(&self) -> u64 {
        self.inner.shared.dropped.load(Ordering::Relaxed)
    }

    /// Times the hooks were reinstalled because Windows may have removed
    /// them: the `GetLastInputInfo` watchdog's requests plus the hook
    /// thread's own stall reinstalls.
    pub fn reinstalls(&self) -> u64 {
        self.inner.shared.reinstalls.load(Ordering::Relaxed)
    }

    /// Tests only: blocks the hook thread for `ms`, as a starved thread
    /// would be.
    #[cfg(test)]
    pub(crate) fn stall_hook_thread_for_test(&self, ms: u32) -> Result<(), PlatformError> {
        self.inner.post_with(MSG_TEST_STALL, ms as usize)
    }

    /// Removes the hooks and ends the hook and watchdog threads. Blocks until
    /// they have ended. Idempotent.
    pub fn shutdown(&self) {
        self.inner.shutdown();
    }
}

/// See [`crate::platform::start_input`].
pub fn start_input(consumer: Thread) -> Result<(InputHandle, InputReceiver), PlatformError> {
    let stored = CONSUMER.get_or_init(|| consumer.clone());
    if stored.id() != consumer.id() {
        return Err(PlatformError::InvalidInput(
            "the input consumer thread cannot change while the app runs",
        ));
    }
    if RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(PlatformError::AlreadyRunning("input hook"));
    }
    let shared = SHARED.get_or_init(|| Arc::new(InputShared::new())).clone();
    shared.clear();
    shared.paused.store(false, Ordering::Release);

    let (tx, rx) = sync_channel(1);
    let thread_shared = shared.clone();
    let join = thread::Builder::new()
        .name("keystage-hooks".into())
        .spawn(move || hook_thread(thread_shared, tx))
        .map_err(|source| {
            RUNNING.store(false, Ordering::Release);
            PlatformError::Spawn {
                name: "input hook",
                source,
            }
        })?;
    let tid = match rx.recv_timeout(START_TIMEOUT) {
        Ok(Ok(tid)) => tid,
        Ok(Err(e)) => {
            let _ = join.join();
            return Err(e);
        }
        // The thread removes its hooks and clears RUNNING itself once it
        // finds nobody waiting.
        Err(RecvTimeoutError::Timeout) => return Err(PlatformError::StartTimeout("input hook")),
        Err(RecvTimeoutError::Disconnected) => {
            let _ = join.join();
            RUNNING.store(false, Ordering::Release);
            return Err(PlatformError::ThreadExited("input hook"));
        }
    };

    let stopping = Arc::new(AtomicBool::new(false));
    let wd_shared = shared.clone();
    let wd_stopping = stopping.clone();
    let watchdog_thread = match thread::Builder::new()
        .name("keystage-hook-watchdog".into())
        .spawn(move || watchdog(wd_shared, tid, wd_stopping))
    {
        Ok(h) => Some(h),
        Err(e) => {
            // The hooks work without it; they just will not be reinstalled.
            tracing::warn!("the input hook watchdog could not start: {e}");
            None
        }
    };

    let inner = Arc::new(Inner {
        shared: shared.clone(),
        tid,
        hook_thread: Mutex::new(Some(join)),
        watchdog: Mutex::new(watchdog_thread),
        stopping,
    });
    let receiver = InputReceiver::new(
        shared,
        &super::PASSWORD_HOLD,
        os::tick_count64,
        layout::fill_scan,
        process::held_modifiers_if_readable,
    );
    Ok((InputHandle { inner }, receiver))
}
