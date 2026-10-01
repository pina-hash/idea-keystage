//! The monitor list: `EnumDisplayMonitors` plus `GetMonitorInfoW`
//! (`MONITORINFOEXW` for the device name) plus `GetDpiForMonitor`, and the
//! monitors' friendly names from the display configuration API.
//!
//! Rectangles are physical pixels because the process is per-monitor DPI
//! aware v2 from its manifest ("The application sees all displays in
//! physical pixels", High DPI Desktop Application Development). There is no
//! window per monitor here, so `GetDpiForWindow` cannot be used; Microsoft's
//! GetDpiForMonitor page says a per-monitor aware caller gets "The actual
//! DPI value set by the user for that display" (its note against
//! per-monitor aware callers contradicts that table; see
//! `.work/research/windows.md`, "Claims that were wrong" 6). Sources read
//! 2026-10-01: EnumDisplayMonitors, GetMonitorInfoW, MONITORINFOEXW,
//! GetDpiForMonitor, QueryDisplayConfig, DisplayConfigGetDeviceInfo,
//! DISPLAYCONFIG_TARGET_DEVICE_NAME.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use keystage_core::ipc::Monitor;
use windows::core::BOOL;
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

use super::super::display::{label_monitors, scale_from_dpi, sort_monitors, wide_to_string};
use super::super::PlatformError;

/// Lists the monitors, primary first, with distinct names.
pub fn list() -> Result<Vec<Monitor>, PlatformError> {
    let mut found = enumerate()?;
    let names = friendly_names();
    for m in &mut found {
        if let Some(name) = names.get(&m.id) {
            m.name = name.clone();
        }
    }
    label_monitors(&mut found);
    Ok(found)
}

/// The monitors' device names, rectangles, scales and primary flag, without
/// names (no display configuration query), in [`list`]'s order. Cheap
/// enough for the watcher's 500 ms poll; a resolution, arrangement, scaling
/// or primary-monitor change alters it. `None` when enumeration fails.
pub(crate) fn layout_signature() -> Option<Vec<Monitor>> {
    enumerate().ok()
}

/// `EnumDisplayMonitors` plus `GetMonitorInfoW` and `GetDpiForMonitor`,
/// sorted, without empty or duplicate entries, names left empty.
fn enumerate() -> Result<Vec<Monitor>, PlatformError> {
    let mut found: Vec<Monitor> = Vec::new();
    // SAFETY: the callback runs synchronously on this thread and only uses
    // `found` through the pointer passed in `dwdata`.
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_proc),
            LPARAM(&mut found as *mut Vec<Monitor> as isize),
        )
    };
    if !ok.as_bool() {
        return Err(PlatformError::last_os_error("EnumDisplayMonitors"));
    }
    // Mirroring pseudo-monitors and duplicates add nothing.
    found.retain(|m| m.width > 0 && m.height > 0);
    sort_monitors(&mut found);
    found.dedup_by(|a, b| a.id == b.id);
    Ok(found)
}

unsafe extern "system" fn enum_proc(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let ptr = data.0 as *mut Vec<Monitor>;
        if ptr.is_null() {
            return;
        }
        // SAFETY: `list` passed a pointer to its live Vec, used only here.
        let found = unsafe { &mut *ptr };
        if let Some(m) = describe(monitor) {
            found.push(m);
        }
    }));
    BOOL::from(true)
}

fn describe(monitor: HMONITOR) -> Option<Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: a MONITORINFOEXW with its size set, passed as MONITORINFO as
    // the API documents.
    let ok = unsafe {
        GetMonitorInfoW(
            monitor,
            &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
    };
    if !ok.as_bool() {
        return None;
    }
    let r = info.monitorInfo.rcMonitor;
    let width = u32::try_from(r.right.saturating_sub(r.left)).unwrap_or(0);
    let height = u32::try_from(r.bottom.saturating_sub(r.top)).unwrap_or(0);
    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    // SAFETY: both out pointers are valid.
    let dpi = match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
    {
        Ok(()) => dpi_x,
        Err(_) => 0,
    };
    Some(Monitor {
        id: wide_to_string(&info.szDevice),
        name: String::new(),
        x: r.left,
        y: r.top,
        width,
        height,
        scale: scale_from_dpi(dpi),
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

/// GDI device name (`\\.\DISPLAY1`) to the monitor's friendly name (from
/// its EDID, e.g. the model name). Empty when the display configuration API
/// fails; callers fall back to `Display N`.
fn friendly_names() -> HashMap<String, String> {
    let mut names = HashMap::new();
    let Some(paths) = active_paths() else {
        return names;
    };
    for p in &paths {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: p.sourceInfo.adapterId,
                id: p.sourceInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: the header describes the enclosing structure and its size.
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
            continue;
        }
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                adapterId: p.targetInfo.adapterId,
                id: p.targetInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: as above.
        if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != 0 {
            continue;
        }
        let gdi = wide_to_string(&source.viewGdiDeviceName);
        let friendly = wide_to_string(&target.monitorFriendlyDeviceName);
        let friendly = friendly.trim();
        if !gdi.is_empty() && !friendly.is_empty() {
            // Duplicate (cloned) displays: the first path wins.
            names.entry(gdi).or_insert_with(|| friendly.to_string());
        }
    }
    names
}

fn active_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // The configuration can change between the two calls; retry a little.
    for _ in 0..3 {
        let (mut n_paths, mut n_modes) = (0u32, 0u32);
        // SAFETY: both out pointers are valid.
        let r = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_paths, &mut n_modes)
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_paths as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_modes as usize];
        // SAFETY: the arrays hold the counts passed in.
        let r = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut n_paths,
                paths.as_mut_ptr(),
                &mut n_modes,
                modes.as_mut_ptr(),
                None,
            )
        };
        if r == ERROR_SUCCESS {
            paths.truncate(n_paths as usize);
            return Some(paths);
        }
        if r != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
    }
    None
}
