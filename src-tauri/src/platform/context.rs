//! What the watcher reports about the desktop, and the pure decisions behind
//! it: the fail-closed password gate, the classic Edit control check, the
//! foreground classification and executable names.
//!
//! The watcher thread (`platform/windows/watcher.rs`) sends [`ContextEvent`]
//! values over a crossbeam channel and unparks the consumer.

use std::sync::Arc;

use keystage_core::{InputEvent, Mod, Side};

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
    /// Modifiers held right now: sent on every foreground change to a
    /// readable window, on unlock, and at start. Forward
    /// `to_input_event()` to the processor.
    Modifiers(ModifierSnapshot),
    /// A password field gained (`true`) or lost (`false`) the keyboard
    /// focus. While true, suppress everything (`Suppress::Password`). Sent
    /// once at start and then only on change.
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
    /// The check could not classify the focus in time. The gate keeps its
    /// previous state (fail closed).
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

/// Combines the two password checks, failing closed.
///
/// * The Win32 check (`GetGUIThreadInfo` focus is an Edit control with
///   `ES_PASSWORD`) sees classic controls only.
/// * UI Automation `IsPassword` sees browser, WPF, UWP and Electron fields,
///   as far as the target app's accessibility support goes.
///
/// A password is active while either check last said so. An `Unknown`
/// verdict changes nothing, so a focus change that cannot be classified in
/// time keeps the previous state rather than briefly showing keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PasswordGate {
    win32: bool,
    uia: bool,
}

impl PasswordGate {
    pub fn new() -> PasswordGate {
        PasswordGate::default()
    }

    pub fn active(&self) -> bool {
        self.win32 || self.uia
    }

    /// Applies a Win32 check result; returns the new combined state when it
    /// changed.
    pub fn on_win32(&mut self, v: Verdict) -> Option<bool> {
        let before = self.active();
        apply(&mut self.win32, v);
        changed(before, self.active())
    }

    /// Applies a UI Automation result; returns the new combined state when
    /// it changed.
    pub fn on_uia(&mut self, v: Verdict) -> Option<bool> {
        let before = self.active();
        apply(&mut self.uia, v);
        changed(before, self.active())
    }
}

fn apply(slot: &mut bool, v: Verdict) {
    match v {
        Verdict::Password => *slot = true,
        Verdict::NotPassword => *slot = false,
        Verdict::Unknown => {}
    }
}

fn changed(before: bool, after: bool) -> Option<bool> {
    (before != after).then_some(after)
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

    #[test]
    fn gate_is_or_of_both_checks() {
        let mut g = PasswordGate::new();
        assert!(!g.active());
        assert_eq!(g.on_win32(Verdict::NotPassword), None);
        assert_eq!(g.on_uia(Verdict::Password), Some(true));
        // The Win32 check does not see browser fields: its "not password"
        // for the browser window must not clear the UI Automation verdict.
        assert_eq!(g.on_win32(Verdict::NotPassword), None);
        assert!(g.active());
        assert_eq!(g.on_uia(Verdict::NotPassword), Some(false));
        assert_eq!(g.on_win32(Verdict::Password), Some(true));
        assert_eq!(g.on_uia(Verdict::NotPassword), None);
        assert_eq!(g.on_win32(Verdict::NotPassword), Some(false));
    }

    #[test]
    fn unknown_keeps_the_previous_state() {
        let mut g = PasswordGate::new();
        assert_eq!(g.on_uia(Verdict::Password), Some(true));
        assert_eq!(g.on_uia(Verdict::Unknown), None);
        assert_eq!(g.on_win32(Verdict::Unknown), None);
        assert!(g.active(), "fail closed");
        assert_eq!(g.on_uia(Verdict::NotPassword), Some(false));
        assert_eq!(g.on_uia(Verdict::Unknown), None);
        assert!(!g.active());
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
