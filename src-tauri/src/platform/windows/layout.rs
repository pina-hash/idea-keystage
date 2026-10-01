//! Keycap legends and typed characters from the foreground keyboard layout.
//!
//! For each key of the keyboard visual's character block
//! ([`keystage_core::keys::CHARACTER_SCANS`]) and the numpad's character
//! keys: `MapVirtualKeyExW(MAPVK_VSC_TO_VK_EX)` gives the layout's virtual
//! key, then `ToUnicodeEx` gives the character in each of the eight Shift,
//! AltGr and Caps Lock states. Every `ToUnicodeEx` call passes flag 0x4
//! ("If bit 2 is set, keyboard state is not changed (Windows 10, version
//! 1607 and newer)"), so building a legend table never eats a dead key the
//! user is in the middle of typing. Sources: MapVirtualKeyExW, ToUnicodeEx,
//! GetKeyboardLayout, GetKeyNameTextW (read 2026-10-01;
//! `.work/research/windows.md` section 5).
//!
//! Never called on the hook thread.

use std::sync::atomic::Ordering;

use keystage_core::keys::{self, vk};
use keystage_core::{LegendProvider, PhysKey, UsLegends};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyNameTextW, GetKeyboardLayout, MapVirtualKeyExW, ToUnicodeEx, HKL, MAPVK_VK_TO_VSC_EX,
    MAPVK_VSC_TO_VK_EX,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::super::legend_table::{
    keyboard_state, keycap_legend, klid_from_hkl, mods_for_index, KeyEntry, LegendTable,
    Translation, NUMPAD_TEXT_KEYS, TEXT_STATES,
};
use super::CURRENT_HKL;

/// `ToUnicodeEx` flag: do not change the keyboard state.
const TOUNICODE_NO_STATE_CHANGE: u32 = 0x4;
/// The Space key's scan code.
const SCAN_SPACE: u32 = 0x39;

/// The keyboard layout of the foreground window's thread, or `None` when
/// there is no foreground window.
pub fn foreground_layout() -> Option<HKL> {
    // SAFETY: plain calls.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let tid = GetWindowThreadProcessId(hwnd, None);
        if tid == 0 {
            return None;
        }
        let hkl = GetKeyboardLayout(tid);
        (!hkl.0.is_null()).then_some(hkl)
    }
}

/// The layout of a given thread (0 is the calling thread).
pub fn thread_layout(tid: u32) -> Option<HKL> {
    // SAFETY: plain call.
    let hkl = unsafe { GetKeyboardLayout(tid) };
    (!hkl.0.is_null()).then_some(hkl)
}

/// The table for the foreground layout, or for this thread's layout when
/// there is no foreground window.
pub fn snapshot() -> LegendTable {
    let hkl = foreground_layout()
        .or_else(|| thread_layout(0))
        .unwrap_or(HKL(std::ptr::null_mut()));
    build_table(hkl)
}

fn translate(code: u16, scan: u32, state: usize, hkl: HKL) -> Translation {
    let ks = keyboard_state(mods_for_index(state));
    let mut buf = [0u16; 16];
    // SAFETY: `ks` is a 256-byte state, `buf` a writable buffer; flag 0x4
    // leaves the kernel keyboard state alone.
    let ret = unsafe {
        ToUnicodeEx(
            u32::from(code),
            scan,
            &ks,
            &mut buf,
            TOUNICODE_NO_STATE_CHANGE,
            layout_param(hkl),
        )
    };
    let t = Translation::decode(ret, &buf);
    if ret < 0 {
        clear_dead_key(hkl);
    }
    t
}

/// Flushes a pending dead key from this thread's translation state. With
/// flag 0x4 nothing should be pending; this keeps the table right even if a
/// system ignored the flag. Bounded, so it cannot loop.
fn clear_dead_key(hkl: HKL) {
    let ks = [0u8; 256];
    let mut buf = [0u16; 8];
    for _ in 0..4 {
        // SAFETY: as in `translate`.
        let ret = unsafe {
            ToUnicodeEx(
                u32::from(vk::SPACE),
                SCAN_SPACE,
                &ks,
                &mut buf,
                TOUNICODE_NO_STATE_CHANGE,
                layout_param(hkl),
            )
        };
        if ret >= 0 {
            break;
        }
    }
}

fn layout_param(hkl: HKL) -> Option<HKL> {
    (!hkl.0.is_null()).then_some(hkl)
}

/// `GetKeyNameTextW` for a key with no character. It uses the calling
/// thread's layout (it takes no HKL), so it is only a fallback.
fn key_name(scan: u16, extended: bool) -> Option<String> {
    let lparam = (i32::from(scan & 0xFF) << 16) | (i32::from(extended) << 24);
    let mut buf = [0u16; 64];
    // SAFETY: `buf` is a writable buffer.
    let n = unsafe { GetKeyNameTextW(lparam, &mut buf) };
    if n <= 0 {
        return None;
    }
    let s = String::from_utf16_lossy(buf.get(..n as usize).unwrap_or(&[]));
    let s = s.trim();
    (!s.is_empty() && !s.chars().any(char::is_control)).then(|| s.to_string())
}

/// Builds the legend table for a layout.
pub fn build_table(hkl: HKL) -> LegendTable {
    let mut entries = Vec::with_capacity(keys::CHARACTER_SCANS.len() + NUMPAD_TEXT_KEYS.len());
    for &scan in keys::CHARACTER_SCANS {
        // SAFETY: plain call.
        let code =
            unsafe { MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, layout_param(hkl)) };
        let Ok(code) = u16::try_from(code) else {
            continue;
        };
        if code == 0 {
            continue;
        }
        let trans: [Translation; TEXT_STATES] =
            std::array::from_fn(|i| translate(code, u32::from(scan), i, hkl));
        let legend = keycap_legend(&trans[0], &trans[1]).or_else(|| {
            if trans[0] == Translation::None && code != vk::SPACE {
                key_name(scan, false)
            } else {
                None
            }
        });
        let dead = trans[0].is_dead();
        entries.push(KeyEntry {
            phys: PhysKey::new(scan, false),
            vk: code,
            keycap: true,
            legend,
            dead,
            text: trans.map(Translation::into_text),
        });
    }
    // Numpad character keys. A layout that types nothing for one (Wine's
    // tables do this; Windows layouts map VK_NUMPAD0..9 directly) falls back
    // to the core's US text for that key, so typed values still read right.
    let us = UsLegends;
    for (code, us_scan, us_extended) in NUMPAD_TEXT_KEYS {
        let phys = scan_for_vk(code, hkl).unwrap_or(PhysKey::new(us_scan, us_extended));
        let text: [Option<String>; TEXT_STATES] = std::array::from_fn(|i| {
            translate(code, u32::from(phys.scan), i, hkl)
                .into_text()
                .or_else(|| us.text(code, phys, mods_for_index(i)))
        });
        entries.push(KeyEntry {
            phys,
            vk: code,
            keycap: false,
            legend: None,
            dead: false,
            text,
        });
    }
    LegendTable::new(klid_from_hkl(hkl.0 as usize), entries)
}

/// The physical key a virtual key sits on in a layout
/// (`MAPVK_VK_TO_VSC_EX`: the high byte is 0xE0 or 0xE1 for extended keys).
fn scan_for_vk(code: u16, hkl: HKL) -> Option<PhysKey> {
    // SAFETY: plain call.
    let sc = unsafe { MapVirtualKeyExW(u32::from(code), MAPVK_VK_TO_VSC_EX, layout_param(hkl)) };
    if sc == 0 {
        return None;
    }
    let prefix = (sc >> 8) & 0xFF;
    Some(PhysKey::new((sc & 0xFF) as u16, prefix == 0xE0))
}

/// Fills the physical key of an injected key event whose scan code is 0,
/// from the foreground layout the watcher last saw. Runs on the pipeline
/// thread (inside `InputReceiver::drain_into`).
pub fn fill_scan(code: u16) -> Option<PhysKey> {
    let raw = CURRENT_HKL.load(Ordering::Acquire);
    scan_for_vk(code, HKL(raw as *mut std::ffi::c_void))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::TextMods;

    #[test]
    fn table_for_this_threads_layout_is_well_formed() {
        let hkl = thread_layout(0).expect("a keyboard layout");
        let t = build_table(hkl);
        let keycaps = t.entries().iter().filter(|e| e.keycap).count();
        assert!(keycaps >= 40, "only {keycaps} character keys mapped");
        assert_eq!(
            t.entries().iter().filter(|e| !e.keycap).count(),
            NUMPAD_TEXT_KEYS.len()
        );
        for e in t.entries() {
            if let Some(l) = &e.legend {
                assert!(!l.trim().is_empty());
                assert!(!l.chars().any(char::is_control));
            }
            for s in e.text.iter().flatten() {
                assert!(!s.chars().any(char::is_control));
            }
        }
        assert!(!t.wire().legends.is_empty());
        assert_eq!(t.layout(), klid_from_hkl(hkl.0 as usize));
        // Numpad digits type ASCII digits with Num Lock's VKs.
        for (i, code) in (vk::NUMPAD0..=vk::NUMPAD9).enumerate() {
            let e = t
                .entries()
                .iter()
                .find(|e| e.vk == code && !e.keycap)
                .expect("numpad entry");
            assert_eq!(
                t.text(code, e.phys, TextMods::default()),
                Some(i.to_string()),
                "numpad {i}"
            );
        }
    }

    #[test]
    fn us_layout_matches_the_core_us_legends() {
        let hkl = thread_layout(0).expect("a keyboard layout");
        if klid_from_hkl(hkl.0 as usize) != "00000409" {
            // Only the US layout has a reference to compare with.
            return;
        }
        let t = build_table(hkl);
        let us = LegendTable::us();
        // Wine's US table puts "<" on the 102nd key; kbdus has "\\" there.
        let wine = super::super::os::running_under_wine();
        let mut got_wire = t.wire();
        let mut want_wire = us.wire();
        if wine {
            got_wire.legends.remove("sc:56");
            want_wire.legends.remove("sc:56");
        }
        assert_eq!(got_wire, want_wire);
        for e in us
            .entries()
            .iter()
            .filter(|e| !(wine && e.phys.scan == 0x56))
        {
            let got = t.entry(e.vk, e.phys).expect("same key");
            // AltGr states differ by design: the core's US provider has no
            // AltGr characters, Windows may return Ctrl+Alt+Space as a space.
            for i in (0..TEXT_STATES).filter(|i| !mods_for_index(*i).altgr) {
                assert_eq!(
                    got.text[i],
                    e.text[i],
                    "vk {:#x} state {i} ({:?})",
                    e.vk,
                    mods_for_index(i)
                );
            }
        }
    }

    #[test]
    fn scan_fill_maps_extended_keys() {
        let hkl = thread_layout(0).expect("a keyboard layout");
        CURRENT_HKL.store(hkl.0 as isize, Ordering::Release);
        assert_eq!(fill_scan(vk::RCONTROL), Some(PhysKey::new(0x1D, true)));
        assert_eq!(fill_scan(vk::LCONTROL), Some(PhysKey::new(0x1D, false)));
        assert_eq!(fill_scan(vk::ESCAPE), Some(PhysKey::new(0x01, false)));
        if !super::super::os::running_under_wine() {
            // Wine omits the E0 prefix that MAPVK_VK_TO_VSC_EX documents.
            assert_eq!(fill_scan(vk::INSERT).map(|p| p.extended), Some(true));
            assert_eq!(fill_scan(vk::NUMPAD0), Some(PhysKey::new(0x52, false)));
        }
    }
}
