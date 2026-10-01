//! Clock and Windows version.

use std::sync::OnceLock;

use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::System::SystemInformation::{GetTickCount, GetTickCount64, OSVERSIONINFOW};

/// Milliseconds since the system started (`GetTickCount64`): monotonic, and
/// its low 32 bits are the `GetTickCount` value the hooks stamp events with.
pub fn tick_count64() -> u64 {
    // SAFETY: no arguments, no preconditions.
    unsafe { GetTickCount64() }
}

/// `GetTickCount`, the 32-bit counter in `KBDLLHOOKSTRUCT.time`,
/// `MSLLHOOKSTRUCT.time` and `LASTINPUTINFO.dwTime`.
pub fn tick_count() -> u32 {
    // SAFETY: no arguments, no preconditions.
    unsafe { GetTickCount() }
}

/// The OS build number, read once with `RtlGetVersion` ("returns version
/// information about the currently running operating system",
/// <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-rtlgetversion>,
/// read 2026-10-01). `GetVersionExW` would report Windows 8 (6.2) because
/// the app manifest declares no supported OS list.
pub fn build() -> Option<u32> {
    static BUILD: OnceLock<Option<u32>> = OnceLock::new();
    *BUILD.get_or_init(|| {
        let mut info = OSVERSIONINFOW {
            dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        // SAFETY: `info` is a valid OSVERSIONINFOW with its size set.
        let status = unsafe { RtlGetVersion(&mut info) };
        (status.0 >= 0 && info.dwBuildNumber > 0).then_some(info.dwBuildNumber)
    })
}

/// True when running on Wine (used only by tests, to keep checks that rely on
/// behavior Wine does not implement strict on real Windows).
#[cfg(test)]
pub(crate) fn running_under_wine() -> bool {
    use windows::core::{s, w};
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    // SAFETY: plain lookups in a module every process has loaded.
    unsafe {
        GetModuleHandleW(w!("ntdll.dll"))
            .ok()
            .and_then(|m| GetProcAddress(m, s!("wine_get_version")))
            .is_some()
    }
}
