//! Clean takes: which windows cover the monitor being recorded
//! (docs/ARCHITECTURE.md section 14). The platform layer lists the
//! top-level windows in z-order (`EnumWindows`, `GetWindowRect`,
//! `DWMWA_CLOAKED`); the rules here decide which of them are occluders.
//! Window titles are never read.

use serde::{Deserialize, Serialize};

use crate::motion::Rect;

/// The shell's taskbars: always on top, never an occluder.
pub const SHELL_CLASSES: &[&str] = &["Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

/// One top-level window as the platform layer saw it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// 0 is the top of the z-order.
    pub z: u32,
    /// The exe file name (`claude.exe`).
    pub process: String,
    pub class: String,
    pub rect: Rect,
    pub topmost: bool,
    pub visible: bool,
    pub cloaked: bool,
    pub minimized: bool,
    /// One of IDEA Keystage's own windows (the overlay, settings).
    pub own: bool,
    /// The foreground window.
    pub foreground: bool,
}

/// A window in front of the app being recorded, over the capture monitor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occluder {
    pub process: String,
    pub z: u32,
    pub rect: Rect,
    pub topmost: bool,
}

/// The windows above the foreground app that overlap `capture`: visible,
/// not cloaked, not minimized, with an area, not IDEA Keystage's own, not
/// the shell taskbar, and not a window of the foreground app itself (its
/// dialogs and menus are part of the tutorial). Without a known foreground
/// window every such window counts (fail closed).
pub fn occluders(windows: &[WindowInfo], capture: &Rect) -> Vec<Occluder> {
    let fg = windows.iter().find(|w| w.foreground);
    let fg_z = fg.map(|w| w.z).unwrap_or(u32::MAX);
    let fg_process = fg.map(|w| w.process.to_ascii_lowercase());
    let mut out: Vec<Occluder> = windows
        .iter()
        .filter(|w| w.z < fg_z)
        .filter(|w| w.visible && !w.cloaked && !w.minimized && !w.own)
        .filter(|w| w.rect.width > 0 && w.rect.height > 0)
        .filter(|w| !SHELL_CLASSES.contains(&w.class.as_str()))
        .filter(|w| fg_process.as_deref() != Some(w.process.to_ascii_lowercase().as_str()))
        .filter(|w| w.rect.intersects(capture))
        .map(|w| Occluder {
            process: w.process.clone(),
            z: w.z,
            rect: w.rect,
            topmost: w.topmost,
        })
        .collect();
    out.sort_by_key(|o| o.z);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    fn w(z: u32, process: &str, class: &str, rect: Rect) -> WindowInfo {
        WindowInfo {
            z,
            process: process.into(),
            class: class.into(),
            rect,
            topmost: false,
            visible: true,
            cloaked: false,
            minimized: false,
            own: false,
            foreground: false,
        }
    }

    fn r(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_floating_window_over_solidworks_is_an_occluder() {
        let mut claude = w(
            1,
            "claude.exe",
            "Chrome_WidgetWin_1",
            r(1500, 800, 380, 240),
        );
        claude.topmost = true;
        let mut sw = w(4, "SLDWORKS.exe", "Afx", r(0, 0, 1920, 1040));
        sw.foreground = true;
        let list = vec![
            w(0, "explorer.exe", "Shell_TrayWnd", r(0, 1040, 1920, 40)),
            claude,
            w(2, "idea-keystage.exe", "Tauri", r(0, 0, 1920, 1080)),
            w(3, "sldworks.exe", "#32770", r(600, 300, 500, 400)),
            sw,
            w(5, "chrome.exe", "Chrome_WidgetWin_1", r(0, 0, 1920, 1080)),
        ];
        let mut list = list;
        list[2].own = true;
        let o = occluders(&list, &MON);
        assert_eq!(o.len(), 1, "{o:?}");
        assert_eq!(o[0].process, "claude.exe");
        assert!(o[0].topmost);
        assert_eq!(o[0].z, 1);
    }

    #[test]
    fn hidden_minimized_cloaked_offscreen_and_lower_windows_do_not_count() {
        let mut fg = w(9, "app.exe", "A", r(0, 0, 1920, 1080));
        fg.foreground = true;
        let mut hidden = w(1, "a.exe", "X", r(0, 0, 10, 10));
        hidden.visible = false;
        let mut cloaked = w(2, "b.exe", "X", r(0, 0, 10, 10));
        cloaked.cloaked = true;
        let mut mini = w(3, "c.exe", "X", r(0, 0, 10, 10));
        mini.minimized = true;
        let other_monitor = w(4, "d.exe", "X", r(-1920, 0, 1920, 1080));
        let empty = w(5, "e.exe", "X", r(5, 5, 0, 10));
        let below = w(10, "f.exe", "X", r(0, 0, 100, 100));
        let second_taskbar = w(
            6,
            "explorer.exe",
            "Shell_SecondaryTrayWnd",
            r(0, 1040, 1920, 40),
        );
        let list = vec![
            hidden,
            cloaked,
            mini,
            other_monitor,
            empty,
            fg,
            below,
            second_taskbar,
        ];
        assert_eq!(occluders(&list, &MON), vec![]);
    }

    #[test]
    fn without_a_known_foreground_every_overlapping_window_counts() {
        let list = vec![
            w(0, "explorer.exe", "Shell_TrayWnd", r(0, 1040, 1920, 40)),
            w(1, "x.exe", "X", r(10, 10, 100, 100)),
            w(2, "y.exe", "Y", r(10, 10, 100, 100)),
        ];
        let o = occluders(&list, &MON);
        assert_eq!(
            o.iter().map(|o| o.process.as_str()).collect::<Vec<_>>(),
            vec!["x.exe", "y.exe"]
        );
    }
}
