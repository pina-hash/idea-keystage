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
//! `UIA_IsPasswordPropertyId`, and then waits for the stop signal. The
//! handler posts each verdict to the watcher's message-only window, where
//! the fail-closed [`crate::platform::PasswordGate`] combines it with the
//! Win32 check. A focus change whose element cannot be read is reported as
//! `Unknown`, which keeps the previous state.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use windows::core::{implement, Ref};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationElement,
    IUIAutomationFocusChangedEventHandler, IUIAutomationFocusChangedEventHandler_Impl,
    UIA_IsPasswordPropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use super::super::{PlatformError, Verdict};

/// Where verdicts go: the watcher window and the message number.
#[derive(Clone)]
struct Target {
    hwnd: isize,
    message: u32,
    /// Cleared before the handler is removed: "It is possible for an event
    /// to be delivered to an event handler after the handler has been
    /// unsubscribed", and the window may be gone by then.
    alive: Arc<AtomicBool>,
}

impl Target {
    fn post(&self, verdict: Verdict) {
        if !self.alive.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: posting to a window of this process; harmless if it is gone.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut std::ffi::c_void)),
                self.message,
                WPARAM(verdict.to_param()),
                LPARAM(0),
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
        self.target.post(verdict);
        Ok(())
    }
}

/// The running UI Automation thread.
pub(super) struct UiaThread {
    stop: Option<SyncSender<()>>,
    join: Option<JoinHandle<()>>,
    alive: Arc<AtomicBool>,
}

impl UiaThread {
    /// Starts the thread, which subscribes in the background so a slow UI
    /// Automation start never delays the watcher. `hwnd` is the watcher's
    /// window, `message` the message number verdicts are posted with, and
    /// `active` is set while the handler is subscribed. Fails only when the
    /// thread cannot be created.
    pub(super) fn spawn(
        hwnd: isize,
        message: u32,
        active: Arc<AtomicBool>,
    ) -> Result<UiaThread, PlatformError> {
        let alive = Arc::new(AtomicBool::new(true));
        let target = Target {
            hwnd,
            message,
            alive: alive.clone(),
        };
        let (stop_tx, stop_rx) = sync_channel::<()>(1);
        let join = thread::Builder::new()
            .name("keystage-uia".into())
            .spawn(move || uia_thread(target, active, stop_rx))
            .map_err(|source| PlatformError::Spawn {
                name: "UI Automation",
                source,
            })?;
        Ok(UiaThread {
            stop: Some(stop_tx),
            join: Some(join),
            alive,
        })
    }

    /// Unsubscribes (on the UI Automation thread) and ends it. Blocks until
    /// the thread has ended. A stop requested while the thread is still
    /// subscribing is honored as soon as it has subscribed.
    pub(super) fn stop(&mut self) {
        self.alive.store(false, Ordering::Release);
        if let Some(tx) = self.stop.take() {
            // Capacity 1 and sent once: never blocks.
            let _ = tx.try_send(());
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

fn subscribe(
    target: &Target,
) -> Result<(IUIAutomation, IUIAutomationFocusChangedEventHandler), PlatformError> {
    // SAFETY: COM calls on this MTA thread.
    unsafe {
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| PlatformError::os("CoCreateInstance(CUIAutomation)", &e))?;
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
        // Classify whatever has the focus right now.
        let initial = match uia.GetFocusedElementBuildCache(&cache) {
            Ok(el) => element_verdict(&el),
            Err(_) => Verdict::Unknown,
        };
        target.post(initial);
        Ok((uia, handler))
    }
}

fn uia_thread(target: Target, active: Arc<AtomicBool>, stop: Receiver<()>) {
    // SAFETY: initializing COM for this new thread.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if let Err(e) = hr.ok() {
        tracing::warn!(
            "UI Automation password detection is unavailable: {}",
            PlatformError::os("CoInitializeEx", &e)
        );
        return;
    }
    let _apartment = ComApartment;
    let (uia, handler) = match subscribe(&target) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("UI Automation password detection is unavailable: {e}");
            return;
        }
    };
    active.store(true, Ordering::Release);
    tracing::info!("UI Automation password detection active");
    // Wait for the stop signal (or for the owner to go away).
    let _ = stop.recv();
    active.store(false, Ordering::Release);
    target.alive.store(false, Ordering::Release);
    // SAFETY: removing on the thread that added, as Microsoft requires.
    if unsafe { uia.RemoveFocusChangedEventHandler(&handler) }.is_err() {
        tracing::warn!("the UI Automation focus handler could not be removed");
    }
    drop(handler);
    drop(uia);
}
