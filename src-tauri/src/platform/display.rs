//! Pure rules for the overlay window and the monitor list: the extended
//! style the overlay must keep, the Windows build that supports excluding a
//! window from capture, DPI scale, monitor labels and order.

use keystage_core::ipc::Monitor;

/// Extended window styles (Extended Window Styles,
/// <https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles>,
/// read 2026-10-01). The Windows build checks them against the `windows`
/// crate.
pub mod ex_style {
    /// "The window should not be painted until siblings beneath the window
    /// ... have been painted"; with `WS_EX_LAYERED` it makes the window
    /// click-through.
    pub const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
    /// "A tool window does not appear in the taskbar or in the dialog that
    /// appears when the user presses ALT+TAB."
    pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    /// "Forces a top-level window onto the taskbar when the window is
    /// visible."
    pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;
    pub const WS_EX_LAYERED: u32 = 0x0008_0000;
    /// "A top-level window created with this style does not become the
    /// foreground window when the user clicks it."
    pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
}

/// The styles the overlay always has.
pub const OVERLAY_EX_STYLE_ON: u32 = ex_style::WS_EX_TOOLWINDOW
    | ex_style::WS_EX_NOACTIVATE
    | ex_style::WS_EX_TRANSPARENT
    | ex_style::WS_EX_LAYERED;

/// The styles the overlay never has.
pub const OVERLAY_EX_STYLE_OFF: u32 = ex_style::WS_EX_APPWINDOW;

/// The overlay's extended style given any proposed one: tool window,
/// non-activating, click-through and layered on; app window off. Every
/// other bit (topmost, no redirection bitmap) is left as proposed.
pub fn force_overlay_ex_style(style: u32) -> u32 {
    (style | OVERLAY_EX_STYLE_ON) & !OVERLAY_EX_STYLE_OFF
}

/// Windows 10 version 2004 (OS build 19041) introduced
/// `WDA_EXCLUDEFROMCAPTURE`; on older builds Windows applies `WDA_MONITOR`
/// and records an empty rectangle instead (SetWindowDisplayAffinity).
pub const EXCLUDE_FROM_CAPTURE_MIN_BUILD: u32 = 19041;

/// True when this build can hide a window from capture.
pub fn capture_exclusion_supported_on(build: Option<u32>) -> bool {
    build.is_some_and(|b| b >= EXCLUDE_FROM_CAPTURE_MIN_BUILD)
}

/// `USER_DEFAULT_SCREEN_DPI`: 100 % scale.
pub const DEFAULT_DPI: u32 = 96;

/// DPI to scale factor (144 dpi gives 1.5). A zero DPI (the call failed)
/// counts as 100 %.
pub fn scale_from_dpi(dpi: u32) -> f64 {
    if dpi == 0 {
        1.0
    } else {
        f64::from(dpi) / f64::from(DEFAULT_DPI)
    }
}

/// A NUL-terminated UTF-16 buffer as a `String` (everything up to the first
/// NUL; invalid units become U+FFFD).
pub fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&u| u == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(buf.get(..end).unwrap_or(&[]))
}

/// The display number in a GDI device name (`\\.\DISPLAY2` gives 2).
pub fn display_number(device: &str) -> Option<u32> {
    let upper = device.to_ascii_uppercase();
    let digits = upper.rsplit("DISPLAY").next()?;
    if digits.is_empty() || digits.len() == upper.len() {
        return None;
    }
    digits.parse().ok()
}

/// A plain label for a monitor without a friendly name: `Display 2`.
pub fn fallback_monitor_name(device: &str, index: usize) -> String {
    match display_number(device) {
        Some(n) => format!("Display {n}"),
        None => format!("Display {}", index + 1),
    }
}

/// Gives every monitor a distinct, readable name: the friendly name when
/// known, otherwise `Display N`; two monitors with the same friendly name
/// (two identical models) get their display number added.
pub fn label_monitors(monitors: &mut [Monitor]) {
    for (i, m) in monitors.iter_mut().enumerate() {
        if m.name.trim().is_empty() {
            m.name = fallback_monitor_name(&m.id, i);
        }
    }
    let names: Vec<String> = monitors.iter().map(|m| m.name.clone()).collect();
    for (i, m) in monitors.iter_mut().enumerate() {
        let shared = names.iter().filter(|n| **n == m.name).count() > 1;
        if shared {
            m.name = format!("{} ({})", m.name, fallback_monitor_name(&m.id, i));
        }
    }
}

/// Orders monitors the way people read them: the primary first, then left
/// to right, then top to bottom.
pub fn sort_monitors(monitors: &mut [Monitor]) {
    monitors.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then(a.x.cmp(&b.x))
            .then(a.y.cmp(&b.y))
            .then(a.id.cmp(&b.id))
    });
}

/// Decides when a change of the monitor configuration has settled, from a
/// signature read on every poll. Docking, undocking and resolution changes
/// pass through intermediate states, so a new signature counts once two
/// polls in a row agree on it. The first reading is the baseline.
#[derive(Debug, Default)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct DisplayChangeWatch<S> {
    reported: Option<S>,
    pending: Option<S>,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl<S: PartialEq> DisplayChangeWatch<S> {
    pub(crate) fn new() -> DisplayChangeWatch<S> {
        DisplayChangeWatch {
            reported: None,
            pending: None,
        }
    }

    /// Feeds one poll's signature. True when a configuration different from
    /// the last one reported has now been read twice in a row.
    pub(crate) fn observe(&mut self, now: S) -> bool {
        match &self.reported {
            None => {
                self.reported = Some(now);
                return false;
            }
            Some(reported) if *reported == now => {
                self.pending = None;
                return false;
            }
            Some(_) => {}
        }
        if self.pending.as_ref() == Some(&now) {
            self.reported = self.pending.take();
            true
        } else {
            self.pending = Some(now);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ex_style::*;

    #[test]
    fn overlay_style_is_forced() {
        // What tao writes for an unowned, topmost, click-through window.
        let tao = WS_EX_APPWINDOW | WS_EX_TRANSPARENT | WS_EX_LAYERED | 0x8 /* TOPMOST */;
        let forced = force_overlay_ex_style(tao);
        assert_eq!(forced & WS_EX_APPWINDOW, 0);
        assert_eq!(forced & OVERLAY_EX_STYLE_ON, OVERLAY_EX_STYLE_ON);
        assert_eq!(forced & 0x8, 0x8, "topmost is kept");
        assert_eq!(force_overlay_ex_style(forced), forced, "idempotent");
        assert_eq!(force_overlay_ex_style(0), OVERLAY_EX_STYLE_ON);
        // No redirection bitmap (0x00200000) survives.
        assert_eq!(
            force_overlay_ex_style(0x0020_0000) & 0x0020_0000,
            0x0020_0000
        );
    }

    #[test]
    fn capture_exclusion_gate() {
        assert!(!capture_exclusion_supported_on(None));
        assert!(!capture_exclusion_supported_on(Some(17763)), "1809");
        assert!(!capture_exclusion_supported_on(Some(18363)), "1909");
        assert!(capture_exclusion_supported_on(Some(19041)), "2004");
        assert!(capture_exclusion_supported_on(Some(22631)));
    }

    #[test]
    fn dpi_scale() {
        assert_eq!(scale_from_dpi(96), 1.0);
        assert_eq!(scale_from_dpi(144), 1.5);
        assert_eq!(scale_from_dpi(192), 2.0);
        assert_eq!(scale_from_dpi(120), 1.25);
        assert_eq!(scale_from_dpi(0), 1.0);
    }

    #[test]
    fn wide_strings() {
        let mut buf = [0u16; 32];
        for (i, u) in r"\\.\DISPLAY1".encode_utf16().enumerate() {
            buf[i] = u;
        }
        assert_eq!(wide_to_string(&buf), r"\\.\DISPLAY1");
        assert_eq!(wide_to_string(&[0x44, 0x45]), "DE");
        assert_eq!(wide_to_string(&[]), "");
        assert_eq!(wide_to_string(&[0]), "");
    }

    #[test]
    fn display_numbers_and_fallback_names() {
        assert_eq!(display_number(r"\\.\DISPLAY1"), Some(1));
        assert_eq!(display_number(r"\\.\DISPLAY12"), Some(12));
        assert_eq!(display_number(r"\\.\display3"), Some(3));
        assert_eq!(display_number(r"\\.\DISPLAY"), None);
        assert_eq!(display_number("MONITOR"), None);
        assert_eq!(fallback_monitor_name(r"\\.\DISPLAY2", 0), "Display 2");
        assert_eq!(fallback_monitor_name("odd", 2), "Display 3");
    }

    fn mon(id: &str, name: &str, x: i32, primary: bool) -> Monitor {
        Monitor {
            id: id.into(),
            name: name.into(),
            x,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
            primary,
        }
    }

    #[test]
    fn labels_are_distinct() {
        let mut v = vec![
            mon(r"\\.\DISPLAY1", "DELL U2720Q", 0, true),
            mon(r"\\.\DISPLAY2", "DELL U2720Q", 1920, false),
            mon(r"\\.\DISPLAY3", "", -1920, false),
            mon(r"\\.\DISPLAY4", "LG HDR 4K", 3840, false),
        ];
        label_monitors(&mut v);
        assert_eq!(v[0].name, "DELL U2720Q (Display 1)");
        assert_eq!(v[1].name, "DELL U2720Q (Display 2)");
        assert_eq!(v[2].name, "Display 3");
        assert_eq!(v[3].name, "LG HDR 4K");
    }

    #[test]
    fn primary_first_then_left_to_right() {
        let mut v = vec![
            mon("c", "C", 1920, false),
            mon("a", "A", -1920, false),
            mon("p", "P", 0, true),
        ];
        sort_monitors(&mut v);
        let ids: Vec<&str> = v.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["p", "a", "c"]);
    }

    #[test]
    fn display_changes_are_reported_once_settled() {
        let mut w = DisplayChangeWatch::new();
        let qhd = vec![mon("a", "", 0, true)];
        let mut hd = qhd.clone();
        hd[0].width = 1280;
        let mut scaled = qhd.clone();
        scaled[0].scale = 1.25;
        assert!(!w.observe(qhd.clone()), "the baseline is not a change");
        assert!(!w.observe(qhd.clone()));
        // A new resolution counts once two polls agree on it.
        assert!(!w.observe(hd.clone()));
        assert!(w.observe(hd.clone()));
        assert!(!w.observe(hd.clone()), "reported once");
        // An intermediate state that does not last is not reported.
        assert!(!w.observe(qhd.clone()));
        assert!(!w.observe(hd.clone()));
        assert!(!w.observe(hd.clone()));
        // Scaling changes count too.
        assert!(!w.observe(scaled.clone()));
        assert!(w.observe(scaled));
    }
}
