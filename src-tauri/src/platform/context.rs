//! What the watcher reports about the desktop, and the pure decisions behind
//! it: the fail-closed password gate (which also drives the input hold,
//! [`super::hold`]), the classic Edit control check, the foreground
//! classification and executable names.
//!
//! The watcher thread (`platform/windows/watcher.rs`) sends [`ContextEvent`]
//! values over a crossbeam channel and unparks the consumer.

use std::sync::Arc;

use keystage_core::{InputEvent, Mod, Side};

use super::hold::{HoldMode, Outcome};
use super::legend_table::LegendTable;

/// How the foreground window was classified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ForegroundKind {
    /// An ordinary app; `exe` names it.
    App,
    /// One of IDEA Keystage's own windows (the settings window).
    OwnProcess,
    /// The foreground process runs elevated (administrator) while IDEA
    /// Keystage does not. Windows does not deliver its input to our hooks
    /// (UIPI), so the app should show a neutral state instead of stale keys.
    Elevated,
    /// No foreground window, or its process could not be identified (a
    /// secure desktop, a protected process, or access denied). The app keeps
    /// its previous profile and exclusion decision; it never treats this as
    /// "not excluded".
    Unknown,
}

/// The foreground window's process.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ForegroundInfo {
    pub kind: ForegroundKind,
    /// The executable's file name, for example `SLDWORKS.exe`, when known.
    /// Only the file name: the folder can contain the user's name.
    pub exe: Option<String>,
    /// Process id, 0 when unknown.
    pub pid: u32,
}

impl ForegroundInfo {
    pub fn unknown() -> ForegroundInfo {
        ForegroundInfo {
            kind: ForegroundKind::Unknown,
            exe: None,
            pid: 0,
        }
    }

    /// True when the hooks most likely see no input for this foreground
    /// (elevated or unidentified). The hook watchdog stays quiet then, and
    /// a modifier resync is not trusted.
    pub fn is_blind(&self) -> bool {
        matches!(
            self.kind,
            ForegroundKind::Elevated | ForegroundKind::Unknown
        )
    }
}

/// Decides the [`ForegroundKind`] from what the lookups found.
///
/// * `pid == 0`: unknown.
/// * our own process id: own process.
/// * the process could not be opened: unknown (an elevated or protected
///   process may refuse `PROCESS_QUERY_LIMITED_INFORMATION`; no Microsoft
///   page says which).
/// * the process token says elevated and ours does not: elevated.
/// * otherwise an ordinary app (an unreadable token is not proof of
///   elevation).
pub fn classify_foreground(
    pid: u32,
    own_pid: u32,
    opened: bool,
    foreground_elevated: Option<bool>,
    self_elevated: Option<bool>,
) -> ForegroundKind {
    if pid == 0 {
        ForegroundKind::Unknown
    } else if pid == own_pid {
        ForegroundKind::OwnProcess
    } else if !opened {
        ForegroundKind::Unknown
    } else if foreground_elevated == Some(true) && self_elevated != Some(true) {
        ForegroundKind::Elevated
    } else {
        ForegroundKind::App
    }
}

/// The file name part of a Win32 image path (`C:\...\SLDWORKS.exe` gives
/// `SLDWORKS.exe`).
pub fn exe_file_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['\\', '/']);
    match trimmed.rfind(['\\', '/']) {
        Some(i) => trimmed.get(i + 1..).unwrap_or(trimmed),
        None => trimmed,
    }
}

/// Modifiers really held, read with `GetAsyncKeyState`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModifierSnapshot {
    pub held: Vec<(Mod, Side)>,
    /// Caps Lock toggle state from `GetKeyState(VK_CAPITAL)`, a seed for
    /// [`keystage_core::Processor::set_caps_lock`]. `None` when unread.
    pub caps_lock: Option<bool>,
    pub t_ms: u64,
}

impl ModifierSnapshot {
    /// The core event to feed to `Processor::handle`.
    pub fn to_input_event(&self) -> InputEvent {
        InputEvent::ModifierResync {
            held: self.held.clone(),
            t_ms: self.t_ms,
        }
    }
}

/// Everything the watcher reports.
#[derive(Clone, Debug)]
pub enum ContextEvent {
    /// The foreground window's process changed (or was first read).
    /// Repeated values for the same process are not sent.
    Foreground(ForegroundInfo),
    /// Modifiers held right now: sent at start and on unlock, when the
    /// desktop is readable. Forward `to_input_event()` to the processor. On
    /// a foreground change the resync arrives instead as an
    /// `InputEvent::ModifierResync` from `InputReceiver::drain_into`, read
    /// after the hook records already queued, so it is ordered with them.
    Modifiers(ModifierSnapshot),
    /// A password field gained (`true`) or lost (`false`) the keyboard
    /// focus, by either check; a focus UI Automation could not classify, or
    /// did not classify in time, counts as a password field. While true,
    /// suppress everything (`Suppress::Password`). Sent once at start and
    /// then only on change. Between a focus change and its classification
    /// the input receiver holds keys back by itself, so this stays false
    /// then and nothing flashes.
    Password { active: bool },
    /// The foreground keyboard layout changed: use this table as the
    /// processor's legend provider and send `table.wire()` to the hosts.
    Layout(Arc<LegendTable>),
    /// The session was locked: call `Processor::release_all`.
    SessionLocked { t_ms: u64 },
    /// The session was unlocked. A `Modifiers` event follows when the
    /// desktop is readable.
    SessionUnlocked { t_ms: u64 },
}

/// One password check's answer for the element that has the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verdict {
    Password,
    NotPassword,
    /// The check could not classify the focus. From the Win32 check it
    /// keeps the previous state; from UI Automation, after a focus change,
    /// it counts as a password (fail closed) and the check is retried.
    Unknown,
}

impl Verdict {
    /// Packs a verdict into a window message parameter.
    pub fn to_param(self) -> usize {
        match self {
            Verdict::NotPassword => 0,
            Verdict::Password => 1,
            Verdict::Unknown => 2,
        }
    }

    /// Unpacks [`Verdict::to_param`]; anything else is `Unknown`.
    pub fn from_param(v: usize) -> Verdict {
        match v {
            0 => Verdict::NotPassword,
            1 => Verdict::Password,
            _ => Verdict::Unknown,
        }
    }
}

/// How many times an `Unknown` UI Automation answer is asked again for one
/// focus ("Clients should handle errors from this method gracefully; for
/// example, by trying the call again", IUIAutomation::GetFocusedElement).
pub const UIA_UNKNOWN_RETRIES: u8 = 3;

/// What the watcher must do after a [`PasswordGate`] update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateStep {
    /// Ask UI Automation to classify the element that has the focus now,
    /// and to answer with this id.
    pub request: Option<u32>,
    /// The focus changed: a hold still pending ends (dropped) and a new one
    /// starts.
    pub new_focus: bool,
    /// The awaited answer said "not a password": what was held for this
    /// focus may be handed out.
    pub release: bool,
    /// The answer was `Unknown`: call [`PasswordGate::on_retry`] after a
    /// short delay.
    pub retry: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiaCheck {
    /// UI Automation is not running: only the Win32 check counts.
    Off,
    /// The focus is classified.
    Known { password: bool },
    /// The focus changed and the answer to `request` has not come yet.
    /// `assume_password` once the wait timed out, or an unsolicited UI
    /// Automation event said "password" meanwhile.
    Waiting { request: u32, assume_password: bool },
}

/// Combines the two password checks and decides the input hold, failing
/// closed (ARCHITECTURE.md section 8, rule 3; DECISIONS item 6).
///
/// * The Win32 check (`GetGUIThreadInfo` focus is an Edit control with
///   `ES_PASSWORD`) is synchronous and sees classic controls only.
/// * UI Automation `IsPassword` sees browser, WPF, UWP and Electron fields,
///   as far as the target app's accessibility support goes, but answers
///   tens of milliseconds after the focus moved. Every focus or foreground
///   change therefore starts a new request; until the answer to *that*
///   request arrives (an answer to an older request is ignored), input is
///   held ([`HoldMode::Hold`]). "Password", "unknown" and "no answer in
///   time" all count as a password ([`HoldMode::Drop`]): nothing typed is
///   shown, and the app suppresses everything while [`Self::active`].
///
/// The gate is pure; the watcher feeds it events, sends the requests,
/// runs the timers and publishes [`Self::mode`] to the input hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasswordGate {
    win32: bool,
    uia: UiaCheck,
    next_request: u32,
    retries_left: u8,
    retry_due: bool,
}

impl Default for PasswordGate {
    fn default() -> Self {
        PasswordGate::new()
    }
}

impl PasswordGate {
    /// Neither check says "password"; UI Automation is not running yet.
    pub fn new() -> PasswordGate {
        PasswordGate {
            win32: false,
            uia: UiaCheck::Off,
            next_request: 1,
            retries_left: UIA_UNKNOWN_RETRIES,
            retry_due: false,
        }
    }

    /// True while a password field has the focus by either check, or the
    /// focus could not be classified: suppress everything
    /// (`ContextEvent::Password { active: true }`).
    pub fn active(&self) -> bool {
        self.win32
            || matches!(
                self.uia,
                UiaCheck::Known { password: true }
                    | UiaCheck::Waiting {
                        assume_password: true,
                        ..
                    }
            )
    }

    /// What the input hold does with input typed now.
    pub fn mode(&self) -> HoldMode {
        if self.active() {
            HoldMode::Drop
        } else if matches!(self.uia, UiaCheck::Waiting { .. }) {
            HoldMode::Hold
        } else {
            HoldMode::Open
        }
    }

    /// What `ContextEvent::Password { active }` should say, given what it
    /// said last: [`Self::active`], and also while input is held for a
    /// focus that left a password field. Moving between two password
    /// fields (or a repeated focus event inside one) then never turns the
    /// suppression off for the moment the new focus is being classified.
    pub fn app_suppressed(&self, previously: bool) -> bool {
        self.active() || (previously && self.mode() == HoldMode::Hold)
    }

    /// The request whose answer is awaited while input is held (the timer
    /// [`Self::on_timeout`] applies to it).
    pub fn awaiting(&self) -> Option<u32> {
        match self.uia {
            UiaCheck::Waiting {
                request,
                assume_password: false,
            } => Some(request),
            _ => None,
        }
    }

    /// True while UI Automation takes part.
    pub fn uia_enabled(&self) -> bool {
        self.uia != UiaCheck::Off
    }

    fn issue(&mut self, assume_password: bool) -> u32 {
        let id = self.next_request;
        self.next_request = self.next_request.wrapping_add(1).max(1);
        self.uia = UiaCheck::Waiting {
            request: id,
            assume_password,
        };
        id
    }

    /// The UI Automation thread was started: classify the current focus,
    /// holding input until it answers.
    pub fn on_uia_started(&mut self) -> GateStep {
        self.uia = UiaCheck::Known { password: false };
        self.on_focus_changed()
    }

    /// UI Automation could not start (or stopped): only the Win32 check
    /// counts from now on. What was held for it is dropped.
    pub fn on_uia_unavailable(&mut self) -> GateStep {
        self.uia = UiaCheck::Off;
        self.retry_due = false;
        GateStep::default()
    }

    /// A focus or foreground change: the previous classification no longer
    /// applies.
    pub fn on_focus_changed(&mut self) -> GateStep {
        if self.uia == UiaCheck::Off {
            return GateStep::default();
        }
        self.retries_left = UIA_UNKNOWN_RETRIES;
        self.retry_due = false;
        GateStep {
            request: Some(self.issue(false)),
            new_focus: true,
            ..GateStep::default()
        }
    }

    /// The Win32 check's result. `Unknown` keeps its previous state.
    pub fn on_win32(&mut self, v: Verdict) -> GateStep {
        match v {
            Verdict::Password => self.win32 = true,
            Verdict::NotPassword => self.win32 = false,
            Verdict::Unknown => {}
        }
        GateStep::default()
    }

    /// UI Automation's answer to `request`. Answers to anything but the
    /// awaited request are ignored: they may describe an element that no
    /// longer has the focus.
    pub fn on_uia_answer(&mut self, request: u32, v: Verdict) -> GateStep {
        match self.uia {
            UiaCheck::Waiting { request: r, .. } if r == request => {}
            _ => return GateStep::default(),
        }
        self.uia = UiaCheck::Known {
            password: v != Verdict::NotPassword,
        };
        let retry = v == Verdict::Unknown && self.retries_left > 0;
        self.retry_due = retry;
        GateStep {
            release: v == Verdict::NotPassword,
            retry,
            ..GateStep::default()
        }
    }

    /// An unsolicited UI Automation focus event with its element's cached
    /// verdict. It may be late (for an element that lost the focus since),
    /// so it never clears anything by itself: a "password" counts at once,
    /// anything that differs from the current classification starts a new
    /// check.
    pub fn on_uia_event(&mut self, v: Verdict) -> GateStep {
        match self.uia {
            UiaCheck::Off => GateStep::default(),
            UiaCheck::Waiting { request, .. } => {
                if v == Verdict::Password {
                    self.uia = UiaCheck::Waiting {
                        request,
                        assume_password: true,
                    };
                }
                GateStep::default()
            }
            UiaCheck::Known { password } => {
                let differs = match v {
                    Verdict::Password => !password,
                    Verdict::NotPassword => password,
                    Verdict::Unknown => true,
                };
                if !differs {
                    return GateStep::default();
                }
                let step = self.on_focus_changed();
                if v == Verdict::Password {
                    if let UiaCheck::Waiting { request, .. } = self.uia {
                        self.uia = UiaCheck::Waiting {
                            request,
                            assume_password: true,
                        };
                    }
                }
                step
            }
        }
    }

    /// No answer to `request` in time: count it as a password until it
    /// comes.
    pub fn on_timeout(&mut self, request: u32) -> GateStep {
        if let UiaCheck::Waiting {
            request: r,
            assume_password: false,
        } = self.uia
        {
            if r == request {
                self.uia = UiaCheck::Waiting {
                    request,
                    assume_password: true,
                };
            }
        }
        GateStep::default()
    }

    /// The retry delay after an `Unknown` answer passed: ask again, still
    /// counting the focus as a password until the answer.
    pub fn on_retry(&mut self) -> GateStep {
        let due = std::mem::take(&mut self.retry_due);
        if !due || self.uia != (UiaCheck::Known { password: true }) || self.retries_left == 0 {
            return GateStep::default();
        }
        self.retries_left -= 1;
        GateStep {
            request: Some(self.issue(true)),
            ..GateStep::default()
        }
    }

    /// Whether the input hold must start a new epoch after `step`, given
    /// the mode published so far, and how the epoch being left ends (only
    /// a `Hold` epoch needs an outcome). `None`: nothing to publish.
    pub fn hold_transition(
        &self,
        published: HoldMode,
        step: &GateStep,
    ) -> Option<(HoldMode, Outcome)> {
        let mode = self.mode();
        let restart = step.new_focus && mode == HoldMode::Hold;
        if mode == published && !restart {
            return None;
        }
        let outcome = if published == HoldMode::Hold && mode == HoldMode::Open && step.release {
            Outcome::Release
        } else {
            Outcome::Drop
        };
        Some((mode, outcome))
    }
}

/// `ES_PASSWORD` (Edit Control Styles): "Displays an asterisk (*) for each
/// character typed into the edit control." Setting a password character with
/// `EM_SETPASSWORDCHAR` sets this style too.
pub const ES_PASSWORD: u32 = 0x0020;

/// True for window classes that are, or are built on, the Edit control:
/// `Edit`, the rich edit classes (`RichEdit20W`, `RICHEDIT50W`), Windows
/// Forms text boxes (`WindowsForms10.EDIT.app...`) and VCL-style edits
/// (`TEdit`, `TMaskEdit`). Matching broadly fails closed: a false match can
/// only suppress, never reveal.
pub fn is_edit_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c == "edit" || c.starts_with("richedit") || c.contains(".edit.") || c.ends_with("edit")
}

/// The Win32 check for one focused window: its class and `GWL_STYLE`.
pub fn win32_verdict(class: &str, style: u32) -> Verdict {
    if is_edit_class(class) && style & ES_PASSWORD != 0 {
        Verdict::Password
    } else {
        Verdict::NotPassword
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gate with UI Automation running and the first focus classified
    /// as not a password.
    fn running_gate() -> PasswordGate {
        let mut g = PasswordGate::new();
        let first = g.on_uia_started().request.expect("first request");
        g.on_uia_answer(first, Verdict::NotPassword);
        assert_eq!(g.mode(), HoldMode::Open);
        g
    }

    #[test]
    fn gate_is_or_of_both_checks() {
        let mut g = running_gate();
        assert!(!g.active());
        g.on_win32(Verdict::Password);
        assert!(g.active());
        assert_eq!(g.mode(), HoldMode::Drop);
        // The Win32 check does not see browser fields: its "not password"
        // for the browser window must not clear the UI Automation verdict.
        g.on_win32(Verdict::NotPassword);
        let r = g.on_focus_changed().request.expect("request");
        g.on_uia_answer(r, Verdict::Password);
        g.on_win32(Verdict::NotPassword);
        assert!(g.active());
        let r = g.on_focus_changed().request.expect("request");
        g.on_uia_answer(r, Verdict::NotPassword);
        assert!(!g.active());
        // A Win32 "unknown" keeps its previous state.
        g.on_win32(Verdict::Password);
        g.on_win32(Verdict::Unknown);
        assert!(g.active(), "fail closed");
    }

    #[test]
    fn input_is_held_from_start_until_the_first_answer() {
        let mut g = PasswordGate::new();
        assert_eq!(g.mode(), HoldMode::Open, "no UI Automation yet");
        let step = g.on_uia_started();
        assert!(step.new_focus);
        let first = step.request.expect("classify the current focus");
        assert_eq!(g.mode(), HoldMode::Hold);
        assert!(!g.active(), "held, not suppressed: no indicator flashes");
        assert_eq!(g.awaiting(), Some(first));
        let step = g.on_uia_answer(first, Verdict::NotPassword);
        assert!(step.release);
        assert_eq!(g.mode(), HoldMode::Open);
        assert_eq!(g.awaiting(), None);
    }

    #[test]
    fn a_focus_change_holds_until_its_own_answer() {
        let mut g = running_gate();
        let r1 = g.on_focus_changed().request.expect("r1");
        assert_eq!(g.mode(), HoldMode::Hold);
        let r2 = g.on_focus_changed().request.expect("r2");
        assert_ne!(r1, r2);
        // A late "not a password" for the previous focus is not taken for
        // the new one.
        assert_eq!(
            g.on_uia_answer(r1, Verdict::NotPassword),
            GateStep::default()
        );
        assert_eq!(g.mode(), HoldMode::Hold);
        g.on_uia_answer(r2, Verdict::Password);
        assert_eq!(g.mode(), HoldMode::Drop);
        assert!(g.active());
    }

    #[test]
    fn unknown_after_a_focus_change_suppresses_and_is_retried() {
        let mut g = running_gate();
        let r = g.on_focus_changed().request.expect("request");
        let step = g.on_uia_answer(r, Verdict::Unknown);
        assert!(g.active(), "fail closed");
        assert!(step.retry && !step.release);
        let mut retries = 0;
        while let Some(again) = g.on_retry().request {
            retries += 1;
            assert!(g.active(), "still suppressed while asking again");
            g.on_uia_answer(again, Verdict::Unknown);
        }
        assert_eq!(retries, UIA_UNKNOWN_RETRIES);
        assert!(g.active(), "stays suppressed until the focus moves");
        // A readable answer after a retry clears it.
        let r = g.on_focus_changed().request.expect("request");
        g.on_uia_answer(r, Verdict::Unknown);
        let again = g.on_retry().request.expect("retry");
        g.on_uia_answer(again, Verdict::NotPassword);
        assert!(!g.active());
        // A retry with nothing due does nothing.
        assert_eq!(g.on_retry(), GateStep::default());
    }

    #[test]
    fn no_answer_in_time_counts_as_a_password() {
        let mut g = running_gate();
        let r = g.on_focus_changed().request.expect("request");
        assert_eq!(g.awaiting(), Some(r));
        g.on_timeout(r);
        assert!(g.active());
        assert_eq!(g.mode(), HoldMode::Drop);
        assert_eq!(g.awaiting(), None);
        // The late answer still decides.
        g.on_uia_answer(r, Verdict::NotPassword);
        assert!(!g.active());
        // A stale timer does nothing.
        let r2 = g.on_focus_changed().request.expect("request");
        g.on_timeout(r);
        assert_eq!(g.mode(), HoldMode::Hold);
        g.on_uia_answer(r2, Verdict::NotPassword);
    }

    #[test]
    fn unsolicited_events_never_clear_anything() {
        let mut g = running_gate();
        // Same classification: nothing to do.
        assert_eq!(g.on_uia_event(Verdict::NotPassword), GateStep::default());
        // "Password" counts at once and is confirmed by a request (a focus
        // change the WinEvent hook did not see, our own settings window).
        let step = g.on_uia_event(Verdict::Password);
        let r = step.request.expect("confirm");
        assert!(g.active());
        // While waiting, "not a password" from an event is ignored ...
        g.on_uia_event(Verdict::NotPassword);
        assert!(g.active());
        // ... only the answer to the request decides.
        g.on_uia_answer(r, Verdict::NotPassword);
        assert!(!g.active());
        // While waiting without an assumption, an event's "password" counts.
        g.on_focus_changed();
        assert_eq!(g.mode(), HoldMode::Hold);
        g.on_uia_event(Verdict::Password);
        assert_eq!(g.mode(), HoldMode::Drop);
        // Leaving a password field by an event starts a check (held).
        let mut g = running_gate();
        let r = g.on_focus_changed().request.expect("request");
        g.on_uia_answer(r, Verdict::Password);
        let step = g.on_uia_event(Verdict::NotPassword);
        assert!(step.request.is_some());
        assert_eq!(g.mode(), HoldMode::Hold);
    }

    #[test]
    fn without_ui_automation_only_the_win32_check_counts() {
        let mut g = PasswordGate::new();
        g.on_uia_started();
        assert_eq!(g.mode(), HoldMode::Hold);
        let step = g.on_uia_unavailable();
        assert!(!step.release, "what was held for it is dropped");
        assert!(!g.uia_enabled());
        assert_eq!(g.mode(), HoldMode::Open);
        assert_eq!(g.on_focus_changed(), GateStep::default());
        assert_eq!(g.on_uia_event(Verdict::Password), GateStep::default());
        g.on_win32(Verdict::Password);
        assert!(g.active());
    }

    #[test]
    fn suppression_stays_on_between_two_password_fields() {
        let mut g = running_gate();
        assert!(!g.app_suppressed(false));
        // Into a password field.
        let r = g.on_focus_changed().request.expect("request");
        assert!(!g.app_suppressed(false), "held, not suppressed");
        g.on_uia_answer(r, Verdict::Password);
        assert!(g.app_suppressed(false));
        // On to the confirmation field: held, and still suppressed.
        let r = g.on_focus_changed().request.expect("request");
        assert_eq!(g.mode(), HoldMode::Hold);
        assert!(g.app_suppressed(true));
        g.on_uia_answer(r, Verdict::Password);
        assert!(g.app_suppressed(true));
        // Out to an ordinary field: off once it is classified.
        let r = g.on_focus_changed().request.expect("request");
        assert!(g.app_suppressed(true));
        g.on_uia_answer(r, Verdict::NotPassword);
        assert!(!g.app_suppressed(true));
    }

    #[test]
    fn hold_transitions() {
        use HoldMode::*;
        let mut g = running_gate();
        assert_eq!(g.hold_transition(Open, &GateStep::default()), None);
        let step = g.on_focus_changed();
        assert_eq!(g.hold_transition(Open, &step), Some((Hold, Outcome::Drop)));
        // A newer focus while holding: a new epoch, the old one dropped.
        let step = g.on_focus_changed();
        assert_eq!(g.hold_transition(Hold, &step), Some((Hold, Outcome::Drop)));
        let r = g.awaiting().expect("awaiting");
        let step = g.on_uia_answer(r, Verdict::NotPassword);
        assert_eq!(
            g.hold_transition(Hold, &step),
            Some((Open, Outcome::Release))
        );
        // "Not a password" from the UI Automation answer, but the Win32
        // check says password: dropped.
        let step = g.on_focus_changed();
        assert_eq!(g.hold_transition(Open, &step), Some((Hold, Outcome::Drop)));
        g.on_win32(Verdict::Password);
        assert_eq!(
            g.hold_transition(Hold, &GateStep::default()),
            Some((Drop, Outcome::Drop))
        );
        let r = g.awaiting().expect("UI Automation is still asked");
        let step = g.on_uia_answer(r, Verdict::NotPassword);
        assert_eq!(g.hold_transition(Drop, &step), None, "still Drop");
    }

    /// PRIV-1 end to end on the pure parts: the watcher's gate decisions,
    /// the shared hold word the hook stamps records with, and the
    /// receiver's filter. A password manager types Tab and the password
    /// 10 ms per key; UI Automation's "password" verdict comes 50 ms after
    /// the focus moved. Nothing of the password may come out, and the app
    /// is told to suppress.
    #[test]
    fn auto_typed_password_before_a_late_verdict_shows_nothing() {
        use crate::platform::hold::{HoldFilter, PasswordHold};
        use keystage_core::{InputEvent, KeyInput, PhysKey};

        struct World {
            gate: PasswordGate,
            hold: PasswordHold,
            published: HoldMode,
            filter: HoldFilter,
            out: Vec<InputEvent>,
            suppressed: bool,
        }
        impl World {
            fn apply(&mut self, step: GateStep) {
                if let Some((mode, outcome)) = self.gate.hold_transition(self.published, &step) {
                    self.hold.publish(mode, outcome);
                    self.published = mode;
                }
                self.suppressed = self.gate.active();
                self.filter.flush(&self.hold, &mut self.out);
            }
            fn key(&mut self, code: u16, scan: u16, t: u64) {
                for down in [true, false] {
                    let ev = InputEvent::Key(KeyInput {
                        vk: code,
                        phys: PhysKey::new(scan, false),
                        down,
                        injected: true,
                        t_ms: t + u64::from(!down),
                    });
                    let stamp = self.hold.stamp();
                    self.filter.push(&self.hold, stamp, ev, &mut self.out);
                }
            }
        }
        let mut w = World {
            gate: running_gate(),
            hold: PasswordHold::new(),
            published: HoldMode::Open,
            filter: HoldFilter::new(),
            out: Vec::new(),
            suppressed: false,
        };
        // User name, then Tab (shown).
        w.key(0x55, 0x16, 0);
        w.key(0x09, 0x0F, 10);
        // EVENT_OBJECT_FOCUS for the password box arrives.
        let step = w.gate.on_focus_changed();
        let request = step.request.expect("request");
        w.apply(step);
        assert!(!w.suppressed, "held, not suppressed");
        // The password, 10 ms per key, before the verdict.
        for (i, (code, scan)) in [(0x53, 0x1F), (0x33, 0x04), (0x43, 0x2E), (0x52, 0x13)]
            .into_iter()
            .enumerate()
        {
            w.key(code, scan, 20 + 10 * i as u64);
        }
        // A late answer for the previous focus changes nothing.
        let stale = w
            .gate
            .on_uia_answer(request.wrapping_sub(1), Verdict::NotPassword);
        w.apply(stale);
        // The verdict for the password box.
        let step = w.gate.on_uia_answer(request, Verdict::Password);
        w.apply(step);
        assert!(w.suppressed);
        // Enter submits (dropped too).
        w.key(0x0D, 0x1C, 80);
        let shown: Vec<u16> = w
            .out
            .iter()
            .filter_map(|e| match e {
                InputEvent::Key(k) if k.down => Some(k.vk),
                _ => None,
            })
            .collect();
        assert_eq!(shown, vec![0x55, 0x09], "only the user name key and Tab");
        assert!(!w.filter.is_holding());

        // The same with "unknown": suppressed as well, nothing shown.
        let before = w.out.len();
        let step = w.gate.on_focus_changed();
        let request = step.request.expect("request");
        w.apply(step);
        w.key(0x53, 0x1F, 200);
        let step = w.gate.on_uia_answer(request, Verdict::Unknown);
        w.apply(step);
        assert!(w.suppressed, "unknown after a focus change suppresses");
        assert_eq!(w.out.len(), before);
    }

    #[test]
    fn verdict_params_round_trip() {
        for v in [Verdict::Password, Verdict::NotPassword, Verdict::Unknown] {
            assert_eq!(Verdict::from_param(v.to_param()), v);
        }
        assert_eq!(Verdict::from_param(99), Verdict::Unknown);
    }

    #[test]
    fn edit_classes() {
        for c in [
            "Edit",
            "EDIT",
            "RichEdit20W",
            "RICHEDIT50W",
            "WindowsForms10.EDIT.app.0.141b42a_r6_ad1",
            "TEdit",
            "TMaskEdit",
        ] {
            assert!(is_edit_class(c), "{c}");
        }
        for c in [
            "Button",
            "Chrome_RenderWidgetHostHWND",
            "SysListView32",
            "ComboBox",
            "",
        ] {
            assert!(!is_edit_class(c), "{c}");
        }
        assert_eq!(win32_verdict("Edit", 0x50010020), Verdict::Password);
        assert_eq!(win32_verdict("Edit", 0x50010080), Verdict::NotPassword);
        assert_eq!(win32_verdict("Button", 0x20), Verdict::NotPassword);
    }

    #[test]
    fn foreground_classification() {
        use ForegroundKind::*;
        assert_eq!(classify_foreground(0, 10, true, None, None), Unknown);
        assert_eq!(classify_foreground(10, 10, true, None, None), OwnProcess);
        assert_eq!(classify_foreground(20, 10, false, None, None), Unknown);
        assert_eq!(
            classify_foreground(20, 10, true, Some(true), Some(false)),
            Elevated
        );
        assert_eq!(
            classify_foreground(20, 10, true, Some(true), None),
            Elevated
        );
        // Both elevated: we see its input.
        assert_eq!(
            classify_foreground(20, 10, true, Some(true), Some(true)),
            App
        );
        assert_eq!(
            classify_foreground(20, 10, true, Some(false), Some(false)),
            App
        );
        assert_eq!(classify_foreground(20, 10, true, None, Some(false)), App);
        assert!(ForegroundInfo::unknown().is_blind());
        let elevated = ForegroundInfo {
            kind: Elevated,
            exe: Some("Taskmgr.exe".into()),
            pid: 4,
        };
        assert!(elevated.is_blind());
        let app = ForegroundInfo {
            kind: App,
            exe: Some("notepad.exe".into()),
            pid: 4,
        };
        assert!(!app.is_blind());
    }

    #[test]
    fn exe_names() {
        assert_eq!(
            exe_file_name(r"C:\Program Files\App\SLDWORKS.exe"),
            "SLDWORKS.exe"
        );
        assert_eq!(exe_file_name("C:/tools/app.exe"), "app.exe");
        assert_eq!(exe_file_name("app.exe"), "app.exe");
        assert_eq!(exe_file_name(r"\\?\C:\x\y.exe"), "y.exe");
        assert_eq!(exe_file_name(r"C:\dir\"), "dir");
        assert_eq!(exe_file_name(""), "");
    }

    #[test]
    fn modifier_snapshot_becomes_a_resync_event() {
        let s = ModifierSnapshot {
            held: vec![(Mod::Ctrl, Side::Left)],
            caps_lock: Some(false),
            t_ms: 9,
        };
        assert_eq!(
            s.to_input_event(),
            InputEvent::ModifierResync {
                held: vec![(Mod::Ctrl, Side::Left)],
                t_ms: 9
            }
        );
    }
}
