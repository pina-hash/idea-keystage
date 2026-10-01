//! UI Automation password detection on its own thread.
//!
//! Microsoft's rules (read 2026-10-01; `.work/research/windows.md` section
//! 2): make UI Automation calls "from a separate thread. ... This thread
//! should not own any windows, and should be a ... Multithreaded Apartment
//! (MTA) model thread"; "A UI Automation client should not use multiple
//! threads to add or remove event handlers"; "Remove event handlers on the
//! same thread"; and "Caching also occurs when you specify a cache request
//! while subscribing to events", so the handler reads `CachedIsPassword`
//! without another cross-process call.
//!
//! The `keystage-uia` thread initializes COM as MTA, creates `CUIAutomation`,
//! registers ONE focus-changed handler with a cache request for
//! `UIA_IsPasswordPropertyId`, and then serves classification requests from
//! the watcher: for each one it reads the element that has the focus now
//! (`GetFocusedElementBuildCache`) and posts the verdict with the request's
//! id to the watcher's message-only window. The watcher asks on every focus
//! and foreground change and holds input until the answer to its latest
//! request arrives, so a late answer about an element that lost the focus
//! is never taken for the new one ([`crate::platform::PasswordGate`]).
//! Requests that queue up while a call is in progress are coalesced to the
//! latest. An element that cannot be read is `Unknown`, which counts as a
//! password. The focus-changed handler's own verdicts are posted as hints:
//! they can arrive late, so the gate only lets them add suppression or
//! start a new request, never clear anything.
//!
//! If the thread cannot initialize COM or subscribe, it tells the watcher,
//! which then relies on the Win32 check alone (`WatcherHandle::uia_active`
//! reports it).
//!
//! UI Automation waits up to 2 s for a provider to return an element and
//! "The default transaction timeout value is 20 seconds"
//! (IUIAutomation2::put_ConnectionTimeout and put_TransactionTimeout, read
//! 2026-10-01). A hung target app would block this thread that long, so
//! both are set to [`CALL_TIMEOUT_MS`]. The watcher's own pending timeout
//! counts the focus as a password meanwhile.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use windows::core::{implement, Interface, Ref};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationCacheRequest, IUIAutomationElement,
    IUIAutomationFocusChangedEventHandler, IUIAutomationFocusChangedEventHandler_Impl,
    UIA_IsPasswordPropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use super::super::{PlatformError, Verdict};

/// Upper bound for one UI Automation call (connection and transaction).
const CALL_TIMEOUT_MS: u32 = 2_000;

/// The messages the thread posts to the watcher window.
#[derive(Clone, Copy, Debug)]
pub(super) struct Messages {
    /// `wParam`: [`Verdict::to_param`], `lParam`: the request id.
    pub(super) answer: u32,
    /// An unsolicited focus event. `wParam`: [`Verdict::to_param`].
    pub(super) event: u32,
    /// UI Automation could not start; no other message follows.
    pub(super) unavailable: u32,
}

/// Where verdicts go: the watcher window and its message numbers.
#[derive(Clone)]
struct Target {
    hwnd: isize,
    messages: Messages,
    /// Cleared before the handler is removed: "It is possible for an event
    /// to be delivered to an event handler after the handler has been
    /// unsubscribed", and the window may be gone by then.
    alive: Arc<AtomicBool>,
}

impl Target {
    fn post(&self, message: u32, wparam: usize, lparam: isize) {
        if !self.alive.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: posting to a window of this process; harmless if it is gone.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut std::ffi::c_void)),
                message,
                WPARAM(wparam),
                LPARAM(lparam),
            )
        };
    }
}

fn element_verdict(element: &IUIAutomationElement) -> Verdict {
    // SAFETY: a COM call on a live element reference.
    match unsafe { element.CachedIsPassword() } {
        Ok(b) if b.as_bool() => Verdict::Password,
        Ok(_) => Verdict::NotPassword,
        Err(_) => Verdict::Unknown,
    }
}

#[implement(IUIAutomationFocusChangedEventHandler)]
struct FocusHandler {
    target: Target,
}

impl IUIAutomationFocusChangedEventHandler_Impl for FocusHandler_Impl {
    fn HandleFocusChangedEvent(
        &self,
        sender: Ref<IUIAutomationElement>,
    ) -> windows::core::Result<()> {
        // Never let a panic unwind into UI Automation.
        let verdict = catch_unwind(AssertUnwindSafe(|| match sender.ok() {
            Ok(element) => element_verdict(element),
            Err(_) => Verdict::Unknown,
        }))
        .unwrap_or(Verdict::Unknown);
        self.target
            .post(self.target.messages.event, verdict.to_param(), 0);
        Ok(())
    }
}

enum Command {
    /// Classify the element that has the focus now; answer with this id.
    Classify(u32),
    Stop,
}

/// The running UI Automation thread.
pub(super) struct UiaThread {
    commands: Option<Sender<Command>>,
    join: Option<JoinHandle<()>>,
    alive: Arc<AtomicBool>,
}

impl UiaThread {
    /// Starts the thread, which subscribes in the background so a slow UI
    /// Automation start never delays the watcher; requests sent meanwhile
    /// are answered once it has subscribed. `hwnd` is the watcher's window,
    /// `messages` the message numbers it posts, and `active` is set while
    /// the handler is subscribed. Fails only when the thread cannot be
    /// created.
    pub(super) fn spawn(
        hwnd: isize,
        messages: Messages,
        active: Arc<AtomicBool>,
    ) -> Result<UiaThread, PlatformError> {
        let alive = Arc::new(AtomicBool::new(true));
        let target = Target {
            hwnd,
            messages,
            alive: alive.clone(),
        };
        let (tx, rx) = channel::<Command>();
        let join = thread::Builder::new()
            .name("keystage-uia".into())
            .spawn(move || uia_thread(target, active, rx))
            .map_err(|source| PlatformError::Spawn {
                name: "UI Automation",
                source,
            })?;
        Ok(UiaThread {
            commands: Some(tx),
            join: Some(join),
            alive,
        })
    }

    /// Asks for the focused element's verdict, answered with `request`.
    /// Never blocks.
    pub(super) fn classify(&self, request: u32) {
        if let Some(tx) = &self.commands {
            let _ = tx.send(Command::Classify(request));
        }
    }

    /// Unsubscribes (on the UI Automation thread) and ends it. Blocks until
    /// the thread has ended: at most one UI Automation call
    /// ([`CALL_TIMEOUT_MS`]) after a stop requested during a call. A stop
    /// requested while the thread is still subscribing is honored as soon
    /// as it has subscribed.
    pub(super) fn stop(&mut self) {
        self.alive.store(false, Ordering::Release);
        if let Some(tx) = self.commands.take() {
            let _ = tx.send(Command::Stop);
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for UiaThread {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Balances `CoInitializeEx` on every exit path.
struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: paired with a successful CoInitializeEx on this thread.
        unsafe { CoUninitialize() };
    }
}

struct Subscription {
    uia: IUIAutomation,
    cache: IUIAutomationCacheRequest,
    handler: IUIAutomationFocusChangedEventHandler,
}

fn subscribe(target: &Target) -> Result<Subscription, PlatformError> {
    // SAFETY: COM calls on this MTA thread.
    unsafe {
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| PlatformError::os("CoCreateInstance(CUIAutomation)", &e))?;
        // Bound every call (Windows 8 and later; best effort).
        if let Ok(uia2) = uia.cast::<IUIAutomation2>() {
            let _ = uia2.SetConnectionTimeout(CALL_TIMEOUT_MS);
            let _ = uia2.SetTransactionTimeout(CALL_TIMEOUT_MS);
        }
        let cache: IUIAutomationCacheRequest = uia
            .CreateCacheRequest()
            .map_err(|e| PlatformError::os("IUIAutomation::CreateCacheRequest", &e))?;
        cache
            .AddProperty(UIA_IsPasswordPropertyId)
            .map_err(|e| PlatformError::os("IUIAutomationCacheRequest::AddProperty", &e))?;
        let handler: IUIAutomationFocusChangedEventHandler = FocusHandler {
            target: target.clone(),
        }
        .into();
        uia.AddFocusChangedEventHandler(&cache, &handler)
            .map_err(|e| PlatformError::os("IUIAutomation::AddFocusChangedEventHandler", &e))?;
        Ok(Subscription {
            uia,
            cache,
            handler,
        })
    }
}

/// The verdict for the element that has the focus now.
fn classify_focus(sub: &Subscription) -> Verdict {
    // SAFETY: COM calls on this MTA thread with live interfaces.
    match unsafe { sub.uia.GetFocusedElementBuildCache(&sub.cache) } {
        Ok(element) => element_verdict(&element),
        // Includes UIA_E_ELEMENTNOTAVAILABLE ("if the focused element is
        // already removed by the time the method returns") and timeouts.
        Err(_) => Verdict::Unknown,
    }
}

fn uia_thread(target: Target, active: Arc<AtomicBool>, commands: Receiver<Command>) {
    // SAFETY: initializing COM for this new thread.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if let Err(e) = hr.ok() {
        tracing::warn!(
            "UI Automation password detection is unavailable: {}",
            PlatformError::os("CoInitializeEx", &e)
        );
        target.post(target.messages.unavailable, 0, 0);
        return;
    }
    let _apartment = ComApartment;
    let sub = match subscribe(&target) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("UI Automation password detection is unavailable: {e}");
            target.post(target.messages.unavailable, 0, 0);
            return;
        }
    };
    active.store(true, Ordering::Release);
    tracing::info!("UI Automation password detection active");
    // Serve requests until the stop signal (or until the owner goes away).
    'serve: while let Ok(first) = commands.recv() {
        let mut latest = None;
        for command in std::iter::once(first).chain(commands.try_iter()) {
            match command {
                Command::Stop => break 'serve,
                Command::Classify(id) => latest = Some(id),
            }
        }
        if let Some(id) = latest {
            let verdict =
                catch_unwind(AssertUnwindSafe(|| classify_focus(&sub))).unwrap_or(Verdict::Unknown);
            target.post(target.messages.answer, verdict.to_param(), id as isize);
        }
    }
    active.store(false, Ordering::Release);
    target.alive.store(false, Ordering::Release);
    // SAFETY: removing on the thread that added, as Microsoft requires.
    if unsafe { sub.uia.RemoveFocusChangedEventHandler(&sub.handler) }.is_err() {
        tracing::warn!("the UI Automation focus handler could not be removed");
    }
    drop(sub);
}
