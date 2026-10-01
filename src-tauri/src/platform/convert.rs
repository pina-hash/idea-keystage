//! Pure conversion from the low-level hooks' raw records to
//! [`keystage_core::InputEvent`] values, plus the clock and watchdog
//! arithmetic the hook layer needs.
//!
//! Nothing here calls the operating system, so it compiles and is tested on
//! every platform. The Windows hook procedures only copy a [`RawEvent`] into a
//! queue; this module turns those records into core events on the pipeline
//! thread.
//!
//! Constants are the documented Win32 values. Sources (read 2026-10-01):
//! KBDLLHOOKSTRUCT <https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct>,
//! MSLLHOOKSTRUCT <https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-msllhookstruct>,
//! WM_MOUSEHWHEEL <https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-mousehwheel>,
//! Virtual-Key Codes <https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>.
//! The Windows build checks each value against the `windows` crate at compile
//! time (`platform/windows/hooks.rs`).

use keystage_core::keys::vk;
use keystage_core::{InputEvent, KeyInput, Mod, MouseButton, PhysKey, Side};

/// Window message numbers the hooks receive as `wParam`.
pub mod msg {
    pub const WM_KEYDOWN: u32 = 0x0100;
    pub const WM_KEYUP: u32 = 0x0101;
    pub const WM_SYSKEYDOWN: u32 = 0x0104;
    pub const WM_SYSKEYUP: u32 = 0x0105;
    pub const WM_MOUSEMOVE: u32 = 0x0200;
    pub const WM_LBUTTONDOWN: u32 = 0x0201;
    pub const WM_LBUTTONUP: u32 = 0x0202;
    pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
    pub const WM_RBUTTONDOWN: u32 = 0x0204;
    pub const WM_RBUTTONUP: u32 = 0x0205;
    pub const WM_RBUTTONDBLCLK: u32 = 0x0206;
    pub const WM_MBUTTONDOWN: u32 = 0x0207;
    pub const WM_MBUTTONUP: u32 = 0x0208;
    pub const WM_MBUTTONDBLCLK: u32 = 0x0209;
    pub const WM_MOUSEWHEEL: u32 = 0x020A;
    pub const WM_XBUTTONDOWN: u32 = 0x020B;
    pub const WM_XBUTTONUP: u32 = 0x020C;
    pub const WM_XBUTTONDBLCLK: u32 = 0x020D;
    /// Not in LowLevelMouseProc's documented list; handled defensively
    /// (`.work/research/windows.md`, "Claims that were wrong" 1).
    pub const WM_MOUSEHWHEEL: u32 = 0x020E;
}

/// `KBDLLHOOKSTRUCT.flags` and `MSLLHOOKSTRUCT.flags` bits.
pub mod flags {
    pub const LLKHF_EXTENDED: u32 = 0x01;
    pub const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;
    /// "Note that bit 4 is also set whenever bit 1 is set."
    pub const LLKHF_INJECTED: u32 = 0x10;
    pub const LLKHF_ALTDOWN: u32 = 0x20;
    pub const LLKHF_UP: u32 = 0x80;
    pub const LLMHF_INJECTED: u32 = 0x01;
    pub const LLMHF_LOWER_IL_INJECTED: u32 = 0x02;
}

/// High word of `mouseData` for `WM_XBUTTON*`.
pub const XBUTTON1: u16 = 0x0001;
pub const XBUTTON2: u16 = 0x0002;

/// `VK_PACKET`: Unicode text sent with `SendInput(KEYEVENTF_UNICODE)`. Its
/// scan code field carries the typed character, never a key position.
pub const VK_PACKET: u16 = 0xE7;

/// Which hook produced a [`RawEvent`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum RawSource {
    #[default]
    Keyboard,
    Mouse,
}

/// The fixed-size, `Copy` record a hook procedure writes into the queue.
///
/// It holds the hook structure's fields unchanged, so the hook procedure does
/// no work beyond the copy. For keyboard records `x`, `y` and `mouse_data`
/// are zero; for mouse records `vk` and `scan` are zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct RawEvent {
    pub source: RawSource,
    /// The hook's `wParam` (`WM_KEYDOWN`, `WM_MOUSEMOVE`, ...).
    pub message: u32,
    /// `KBDLLHOOKSTRUCT.vkCode`.
    pub vk: u32,
    /// `KBDLLHOOKSTRUCT.scanCode`, unmasked (the AltGr fake Ctrl is `0x21D`).
    pub scan: u32,
    /// `KBDLLHOOKSTRUCT.flags` or `MSLLHOOKSTRUCT.flags`.
    pub flags: u32,
    /// The hook's `time` field: `GetTickCount` milliseconds, wraps every
    /// 49.7 days.
    pub time: u32,
    /// `MSLLHOOKSTRUCT.pt`, per-monitor-aware (physical) screen pixels.
    pub x: i32,
    pub y: i32,
    /// `MSLLHOOKSTRUCT.mouseData`.
    pub mouse_data: u32,
}

impl RawEvent {
    /// A keyboard record as the keyboard hook procedure builds it.
    pub fn key(message: u32, vk: u32, scan: u32, flags: u32, time: u32) -> RawEvent {
        RawEvent {
            source: RawSource::Keyboard,
            message,
            vk,
            scan,
            flags,
            time,
            ..RawEvent::default()
        }
    }

    /// A mouse record as the mouse hook procedure builds it.
    pub fn mouse(message: u32, x: i32, y: i32, mouse_data: u32, flags: u32, time: u32) -> RawEvent {
        RawEvent {
            source: RawSource::Mouse,
            message,
            flags,
            time,
            x,
            y,
            mouse_data,
            ..RawEvent::default()
        }
    }
}

/// The signed high word of `mouseData` (wheel delta, X button number).
pub fn high_word(v: u32) -> u16 {
    (v >> 16) as u16
}

/// Converts one raw record into a core input event, or `None` for a record
/// that carries nothing the core uses (an unknown message, a zero wheel
/// delta, an out-of-range virtual key, an X button that is neither 1 nor 2).
///
/// `t_ms` is the rebased timestamp ([`TimeRebaser`]). `fill_scan` supplies a
/// physical key for keyboard records whose scan code is 0 (input injected by
/// virtual key only); on Windows it is `MapVirtualKeyExW(vk,
/// MAPVK_VK_TO_VSC_EX, hkl)` for the foreground layout.
pub fn to_input_event(
    raw: &RawEvent,
    t_ms: u64,
    fill_scan: &dyn Fn(u16) -> Option<PhysKey>,
) -> Option<InputEvent> {
    match raw.source {
        RawSource::Keyboard => key_event(raw, t_ms, fill_scan),
        RawSource::Mouse => mouse_event(raw, t_ms),
    }
}

fn key_event(
    raw: &RawEvent,
    t_ms: u64,
    fill_scan: &dyn Fn(u16) -> Option<PhysKey>,
) -> Option<InputEvent> {
    // "The code must be a value in the range 1 to 254."
    let code = u16::try_from(raw.vk)
        .ok()
        .filter(|v| (1..=254).contains(v))?;
    let down = match raw.message {
        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => true,
        msg::WM_KEYUP | msg::WM_SYSKEYUP => false,
        // Not one of the four documented messages: trust the transition bit.
        _ => raw.flags & flags::LLKHF_UP == 0,
    };
    let extended = raw.flags & flags::LLKHF_EXTENDED != 0;
    let injected = raw.flags & flags::LLKHF_INJECTED != 0;
    let phys = if code == VK_PACKET {
        // The scan field holds the typed character: never let it reach the
        // core as a key position (privacy, and it is not a key).
        PhysKey::new(0, false)
    } else if raw.scan == 0 {
        match fill_scan(code) {
            Some(p) => PhysKey::new(p.scan, p.extended || extended),
            None => PhysKey::new(0, extended),
        }
    } else {
        // Scan codes fit in 16 bits; 0x21D (AltGr's fake Ctrl) is kept whole.
        PhysKey::new((raw.scan & 0xFFFF) as u16, extended)
    };
    Some(InputEvent::Key(KeyInput {
        vk: code,
        phys,
        down,
        injected,
        t_ms,
    }))
}

fn mouse_event(raw: &RawEvent, t_ms: u64) -> Option<InputEvent> {
    let injected = raw.flags & flags::LLMHF_INJECTED != 0;
    let (x, y) = (raw.x, raw.y);
    let button = |button: MouseButton, down: bool| {
        Some(InputEvent::Button {
            button,
            down,
            x,
            y,
            injected,
            t_ms,
        })
    };
    let x_button = || match high_word(raw.mouse_data) {
        XBUTTON1 => Some(MouseButton::X1),
        XBUTTON2 => Some(MouseButton::X2),
        _ => None,
    };
    let delta = || i32::from(high_word(raw.mouse_data) as i16);
    match raw.message {
        msg::WM_MOUSEMOVE => Some(InputEvent::Move { x, y, t_ms }),
        // Low-level hooks do not document double-click messages; if one
        // arrives it is a second press, and the core detects double clicks.
        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONDBLCLK => button(MouseButton::Left, true),
        msg::WM_LBUTTONUP => button(MouseButton::Left, false),
        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONDBLCLK => button(MouseButton::Right, true),
        msg::WM_RBUTTONUP => button(MouseButton::Right, false),
        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONDBLCLK => button(MouseButton::Middle, true),
        msg::WM_MBUTTONUP => button(MouseButton::Middle, false),
        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONDBLCLK => button(x_button()?, true),
        msg::WM_XBUTTONUP => button(x_button()?, false),
        msg::WM_MOUSEWHEEL => {
            let dy = delta();
            (dy != 0).then_some(InputEvent::Wheel {
                dx: 0,
                dy,
                x,
                y,
                t_ms,
            })
        }
        msg::WM_MOUSEHWHEEL => {
            let dx = delta();
            (dx != 0).then_some(InputEvent::Wheel {
                dx,
                dy: 0,
                x,
                y,
                t_ms,
            })
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// Maps a 32-bit `GetTickCount` timestamp onto the 64-bit tick count
/// (`GetTickCount64`, whose low 32 bits are the same counter): the value with
/// those low bits that lies closest to `now64` (at most 24.8 days away).
pub fn unwrap_tick(t32: u32, now64: u64) -> u64 {
    let now32 = now64 as u32;
    let behind = now32.wrapping_sub(t32);
    if behind <= i32::MAX as u32 {
        now64.saturating_sub(u64::from(behind))
    } else {
        now64.saturating_add(u64::from(t32.wrapping_sub(now32)))
    }
}

/// How far a hook timestamp may lag the clock before it is treated as bogus
/// (input injected with its own time stamp) and replaced by "now".
pub const MAX_LAG_MS: u64 = 60_000;

/// How far a hook timestamp may lead the clock (both come from the same
/// counter, so only rounding should ever show here).
pub const MAX_LEAD_MS: u64 = 1_000;

/// Turns hook timestamps into a monotonic `u64` millisecond clock on the
/// same base as [`crate::platform::now_ms`].
#[derive(Clone, Debug, Default)]
pub struct TimeRebaser {
    last: u64,
}

impl TimeRebaser {
    pub fn new() -> TimeRebaser {
        TimeRebaser::default()
    }

    /// Rebases one timestamp. The result never goes backwards, and a
    /// timestamp that is implausibly far from `now64` is replaced by it.
    pub fn rebase(&mut self, t32: u32, now64: u64) -> u64 {
        let mut t = unwrap_tick(t32, now64);
        if t > now64.saturating_add(MAX_LEAD_MS) || t.saturating_add(MAX_LAG_MS) < now64 {
            t = now64;
        }
        if t < self.last {
            t = self.last;
        }
        self.last = t;
        t
    }

    /// A timestamp for "now" that never goes backwards, for events the
    /// platform layer makes itself (a modifier resync).
    pub fn stamp_now(&mut self, now64: u64) -> u64 {
        let t = now64.max(self.last);
        self.last = t;
        t
    }

    /// The last value returned (0 before the first call).
    pub fn last(&self) -> u64 {
        self.last
    }
}

// ---------------------------------------------------------------------------
// Modifier resync
// ---------------------------------------------------------------------------

/// The left/right modifier virtual keys `GetAsyncKeyState` can tell apart,
/// in the order resync reports them.
pub const MODIFIER_VKS: [(u16, Mod, Side); 8] = [
    (vk::LCONTROL, Mod::Ctrl, Side::Left),
    (vk::RCONTROL, Mod::Ctrl, Side::Right),
    (vk::LSHIFT, Mod::Shift, Side::Left),
    (vk::RSHIFT, Mod::Shift, Side::Right),
    (vk::LMENU, Mod::Alt, Side::Left),
    (vk::RMENU, Mod::Alt, Side::Right),
    (vk::LWIN, Mod::Win, Side::Left),
    (vk::RWIN, Mod::Win, Side::Right),
];

/// The modifiers that are down, given a predicate over virtual keys.
pub fn held_modifiers(is_down: impl Fn(u16) -> bool) -> Vec<(Mod, Side)> {
    MODIFIER_VKS
        .iter()
        .filter(|(code, _, _)| is_down(*code))
        .map(|(_, m, s)| (*m, *s))
        .collect()
}

/// `GetAsyncKeyState`: "The most significant bit of the return value is set
/// if the specified key is currently down."
pub fn async_key_down(state: i16) -> bool {
    state < 0
}

/// `GetKeyState`: "If the low-order bit is 1, the key is toggled."
pub fn key_toggled(state: i16) -> bool {
    state & 1 != 0
}

/// The position of a left/right modifier virtual key in [`MODIFIER_VKS`].
pub(crate) fn modifier_index(code: u16) -> Option<usize> {
    MODIFIER_VKS.iter().position(|(c, _, _)| *c == code)
}

/// For this long after a modifier key's last event in the hook stream, the
/// stream, not `GetAsyncKeyState`, says whether that key is held in a
/// resync. "The callback function is called before the asynchronous state
/// of the key is updated" (LowLevelKeyboardProc), so a read made just after
/// the consumer took a key's record can still report the key's old state
/// and would re-press a modifier that was just released (or release one
/// just pressed).
pub(crate) const RESYNC_STREAM_TRUST_MS: u64 = 100;

/// Packs a modifier key's last stream event (`down`, rebased `t_ms`) into one
/// atomic word; 0 means "no event". `t_ms` is milliseconds since boot, far
/// below 2^61.
pub(crate) fn pack_modifier_event(down: bool, t_ms: u64) -> u64 {
    (t_ms << 2) | (u64::from(down) << 1) | 1
}

/// Reverses [`pack_modifier_event`].
pub(crate) fn unpack_modifier_event(word: u64) -> Option<(bool, u64)> {
    (word & 1 != 0).then_some((word & 2 != 0, word >> 2))
}

/// The modifiers to report in a resync: `GetAsyncKeyState`'s answer
/// (`async_held`), except for keys whose last stream event (`recent`,
/// indexed like [`MODIFIER_VKS`]) is at most [`RESYNC_STREAM_TRUST_MS`] old
/// at `now`; for those the stream's state wins. Order follows
/// [`MODIFIER_VKS`].
pub(crate) fn reconcile_held(
    async_held: &[(Mod, Side)],
    recent: &[Option<(bool, u64)>; 8],
    now: u64,
) -> Vec<(Mod, Side)> {
    MODIFIER_VKS
        .iter()
        .zip(recent.iter())
        .filter(|((_, m, s), last)| match last {
            Some((down, t)) if now.saturating_sub(*t) <= RESYNC_STREAM_TRUST_MS => *down,
            _ => async_held.contains(&(*m, *s)),
        })
        .map(|((_, m, s), _)| (*m, *s))
        .collect()
}

// ---------------------------------------------------------------------------
// Hook watchdog
// ---------------------------------------------------------------------------

/// How much later than the hook's last event the system's last input may be
/// before the hooks are presumed removed.
pub const WATCHDOG_THRESHOLD_MS: u32 = 3_000;

/// True when Windows saw input (`GetLastInputInfo`) more than `threshold` ms
/// after the hooks last delivered anything: Windows 7 and later remove a
/// low-level hook that times out "without being called" and do not say so
/// (LowLevelKeyboardProc). Both values are 32-bit tick counts; the
/// comparison is wrap-safe.
pub fn hook_looks_dead(last_input: u32, last_hook: u32, threshold: u32) -> bool {
    let gap = last_input.wrapping_sub(last_hook);
    gap > threshold && gap <= i32::MAX as u32
}

/// Period of the hook thread's own liveness timer.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const STALL_PROBE_MS: u32 = 50;

/// A gap this long between two liveness ticks means the hook thread did not
/// run for most of it. A low-level hook call that waits longer than
/// `LowLevelHooksTimeout` (at most 1000 ms since Windows 10 1709; Microsoft
/// documents no default) makes Windows remove that one hook "without being
/// called", and only the hook that timed out goes. The `GetLastInputInfo`
/// check cannot see a lost keyboard hook while the mouse hook keeps
/// delivering, so the hook thread reinstalls both hooks after any stall this
/// long. Healthy gaps are the period plus timer and tick granularity (about
/// 16 ms each).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const STALL_THRESHOLD_MS: u64 = 150;

/// Measures how long the hook thread went without running, from the ticks
/// of its liveness timer. A stall of `d` ms shows as a gap of at least `d`,
/// so every stall of [`STALL_THRESHOLD_MS`] or more is reported.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct StallProbe {
    last: Option<u64>,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl StallProbe {
    pub(crate) fn new() -> StallProbe {
        StallProbe::default()
    }

    /// Records a timer tick at `now` (milliseconds, monotonic). Returns the
    /// gap since the previous tick when it is at least `threshold`.
    pub(crate) fn tick(&mut self, now: u64, threshold: u64) -> Option<u64> {
        let gap = now.saturating_sub(self.last.replace(now)?);
        (gap >= threshold).then_some(gap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_fill(_: u16) -> Option<PhysKey> {
        None
    }

    fn key(raw: RawEvent) -> KeyInput {
        match to_input_event(&raw, 77, &no_fill) {
            Some(InputEvent::Key(k)) => k,
            other => panic!("expected a key event, got {other:?}"),
        }
    }

    #[test]
    fn key_down_and_up_from_all_four_messages() {
        let a = key(RawEvent::key(msg::WM_KEYDOWN, 0x41, 0x1E, 0, 10));
        assert_eq!(a.vk, 0x41);
        assert_eq!(a.phys, PhysKey::new(0x1E, false));
        assert!(a.down);
        assert!(!a.injected);
        assert_eq!(a.t_ms, 77);
        assert!(
            !key(RawEvent::key(
                msg::WM_KEYUP,
                0x41,
                0x1E,
                flags::LLKHF_UP,
                10
            ))
            .down
        );
        assert!(
            key(RawEvent::key(
                msg::WM_SYSKEYDOWN,
                0xA4,
                0x38,
                flags::LLKHF_ALTDOWN,
                10
            ))
            .down
        );
        assert!(
            !key(RawEvent::key(
                msg::WM_SYSKEYUP,
                0xA4,
                0x38,
                flags::LLKHF_UP,
                10
            ))
            .down
        );
    }

    #[test]
    fn unknown_key_message_falls_back_to_transition_bit() {
        assert!(key(RawEvent::key(0x0102, 0x41, 0x1E, 0, 1)).down);
        assert!(!key(RawEvent::key(0x0102, 0x41, 0x1E, flags::LLKHF_UP, 1)).down);
    }

    #[test]
    fn extended_and_injected_flags() {
        let rctrl = key(RawEvent::key(
            msg::WM_KEYDOWN,
            0xA3,
            0x1D,
            flags::LLKHF_EXTENDED,
            1,
        ));
        assert_eq!(rctrl.phys, PhysKey::new(0x1D, true));
        assert_eq!(rctrl.phys.wire_id(), "sc:E01D");
        let lower_il = flags::LLKHF_LOWER_IL_INJECTED | flags::LLKHF_INJECTED;
        assert!(key(RawEvent::key(msg::WM_KEYDOWN, 0x41, 0x1E, lower_il, 1)).injected);
        assert!(
            key(RawEvent::key(
                msg::WM_KEYDOWN,
                0x41,
                0x1E,
                flags::LLKHF_INJECTED,
                1
            ))
            .injected
        );
        // Right Shift has its own scan code and no extended flag.
        let rshift = key(RawEvent::key(msg::WM_KEYDOWN, 0xA1, 0x36, 0, 1));
        assert_eq!(rshift.phys, PhysKey::new(0x36, false));
    }

    #[test]
    fn altgr_fake_ctrl_scan_code_is_kept_whole() {
        let k = key(RawEvent::key(msg::WM_KEYDOWN, 0xA2, 0x21D, 0, 1));
        assert_eq!(k.phys.scan, 0x21D);
        assert!(keystage_core::keys::is_altgr_fake_ctrl(k.vk, k.phys));
    }

    #[test]
    fn numpad_and_navigation_differ_by_extended_flag() {
        let home = key(RawEvent::key(
            msg::WM_KEYDOWN,
            0x24,
            0x47,
            flags::LLKHF_EXTENDED,
            1,
        ));
        let num_home = key(RawEvent::key(msg::WM_KEYDOWN, 0x24, 0x47, 0, 1));
        assert_eq!(home.phys.wire_id(), "sc:E047");
        assert_eq!(num_home.phys.wire_id(), "sc:47");
        assert_eq!(
            keystage_core::keys::classify(home.vk, home.phys).name,
            "Home"
        );
        assert_eq!(
            keystage_core::keys::classify(num_home.vk, num_home.phys).name,
            "NumHome"
        );
    }

    #[test]
    fn out_of_range_virtual_keys_are_dropped() {
        assert!(
            to_input_event(&RawEvent::key(msg::WM_KEYDOWN, 0, 0x1E, 0, 1), 1, &no_fill).is_none()
        );
        assert!(to_input_event(
            &RawEvent::key(msg::WM_KEYDOWN, 255, 0x1E, 0, 1),
            1,
            &no_fill
        )
        .is_none());
        assert!(to_input_event(
            &RawEvent::key(msg::WM_KEYDOWN, 0x1_0041, 0x1E, 0, 1),
            1,
            &no_fill
        )
        .is_none());
    }

    #[test]
    fn zero_scan_code_is_filled_from_the_layout() {
        let fill = |code: u16| match code {
            0xA3 => Some(PhysKey::new(0x1D, true)),
            0x41 => Some(PhysKey::new(0x1E, false)),
            _ => None,
        };
        let raw = RawEvent::key(msg::WM_KEYDOWN, 0x41, 0, flags::LLKHF_INJECTED, 1);
        match to_input_event(&raw, 5, &fill) {
            Some(InputEvent::Key(k)) => {
                assert_eq!(k.phys, PhysKey::new(0x1E, false));
                assert!(k.injected);
            }
            other => panic!("{other:?}"),
        }
        let raw = RawEvent::key(msg::WM_KEYDOWN, 0xA3, 0, 0, 1);
        match to_input_event(&raw, 5, &fill) {
            Some(InputEvent::Key(k)) => assert_eq!(k.phys, PhysKey::new(0x1D, true)),
            other => panic!("{other:?}"),
        }
        // No fill available: scan 0, extended flag from the record.
        let raw = RawEvent::key(msg::WM_KEYDOWN, 0x2E, 0, flags::LLKHF_EXTENDED, 1);
        match to_input_event(&raw, 5, &fill) {
            Some(InputEvent::Key(k)) => assert_eq!(k.phys, PhysKey::new(0, true)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn vk_packet_never_carries_its_character_as_a_scan_code() {
        // SendInput(KEYEVENTF_UNICODE) for 'S' (0x53): the scan field is the character.
        let fill = |_: u16| Some(PhysKey::new(0x1F, false));
        let raw = RawEvent::key(
            msg::WM_KEYDOWN,
            u32::from(VK_PACKET),
            0x53,
            flags::LLKHF_INJECTED,
            1,
        );
        match to_input_event(&raw, 5, &fill) {
            Some(InputEvent::Key(k)) => {
                assert_eq!(k.vk, VK_PACKET);
                assert_eq!(k.phys, PhysKey::new(0, false));
            }
            other => panic!("{other:?}"),
        }
    }

    fn mouse(message: u32, data: u32, fl: u32) -> Option<InputEvent> {
        to_input_event(
            &RawEvent::mouse(message, -1920, 300, data, fl, 9),
            42,
            &no_fill,
        )
    }

    #[test]
    fn mouse_buttons_and_moves() {
        assert_eq!(
            mouse(msg::WM_MOUSEMOVE, 0, 0),
            Some(InputEvent::Move {
                x: -1920,
                y: 300,
                t_ms: 42
            })
        );
        let cases = [
            (msg::WM_LBUTTONDOWN, MouseButton::Left, true),
            (msg::WM_LBUTTONUP, MouseButton::Left, false),
            (msg::WM_LBUTTONDBLCLK, MouseButton::Left, true),
            (msg::WM_RBUTTONDOWN, MouseButton::Right, true),
            (msg::WM_RBUTTONUP, MouseButton::Right, false),
            (msg::WM_RBUTTONDBLCLK, MouseButton::Right, true),
            (msg::WM_MBUTTONDOWN, MouseButton::Middle, true),
            (msg::WM_MBUTTONUP, MouseButton::Middle, false),
            (msg::WM_MBUTTONDBLCLK, MouseButton::Middle, true),
        ];
        for (m, b, d) in cases {
            assert_eq!(
                mouse(m, 0, 0),
                Some(InputEvent::Button {
                    button: b,
                    down: d,
                    x: -1920,
                    y: 300,
                    injected: false,
                    t_ms: 42
                }),
                "message {m:#x}"
            );
        }
        match mouse(msg::WM_LBUTTONDOWN, 0, flags::LLMHF_INJECTED) {
            Some(InputEvent::Button { injected, .. }) => assert!(injected),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn x_buttons_from_the_high_word() {
        let x1 = u32::from(XBUTTON1) << 16;
        let x2 = u32::from(XBUTTON2) << 16;
        assert!(matches!(
            mouse(msg::WM_XBUTTONDOWN, x1, 0),
            Some(InputEvent::Button {
                button: MouseButton::X1,
                down: true,
                ..
            })
        ));
        assert!(matches!(
            mouse(msg::WM_XBUTTONUP, x2, 0),
            Some(InputEvent::Button {
                button: MouseButton::X2,
                down: false,
                ..
            })
        ));
        assert!(matches!(
            mouse(msg::WM_XBUTTONDBLCLK, x2, 0),
            Some(InputEvent::Button {
                button: MouseButton::X2,
                down: true,
                ..
            })
        ));
        assert_eq!(mouse(msg::WM_XBUTTONDOWN, 3 << 16, 0), None);
        assert_eq!(mouse(msg::WM_XBUTTONDOWN, 0, 0), None);
    }

    #[test]
    fn wheel_deltas_are_signed_and_may_be_fractional() {
        let enc = |d: i16| u32::from(d as u16) << 16;
        assert_eq!(
            mouse(msg::WM_MOUSEWHEEL, enc(120), 0),
            Some(InputEvent::Wheel {
                dx: 0,
                dy: 120,
                x: -1920,
                y: 300,
                t_ms: 42
            })
        );
        assert_eq!(
            mouse(msg::WM_MOUSEWHEEL, enc(-240), 0),
            Some(InputEvent::Wheel {
                dx: 0,
                dy: -240,
                x: -1920,
                y: 300,
                t_ms: 42
            })
        );
        // High-resolution wheels send fractions of WHEEL_DELTA.
        assert_eq!(
            mouse(msg::WM_MOUSEWHEEL, enc(30) | 0xFFFF, 0),
            Some(InputEvent::Wheel {
                dx: 0,
                dy: 30,
                x: -1920,
                y: 300,
                t_ms: 42
            })
        );
        // Horizontal: positive is right.
        assert_eq!(
            mouse(msg::WM_MOUSEHWHEEL, enc(-120), 0),
            Some(InputEvent::Wheel {
                dx: -120,
                dy: 0,
                x: -1920,
                y: 300,
                t_ms: 42
            })
        );
        assert_eq!(mouse(msg::WM_MOUSEWHEEL, 0, 0), None);
        assert_eq!(mouse(msg::WM_MOUSEHWHEEL, 0xFFFF, 0), None);
    }

    #[test]
    fn unknown_mouse_messages_are_dropped() {
        assert_eq!(mouse(0x00A0, 0, 0), None); // WM_NCMOUSEMOVE
        assert_eq!(mouse(0x0100, 0, 0), None);
    }

    #[test]
    fn unwrap_tick_handles_wrap_in_both_directions() {
        let day = 86_400_000u64;
        // Ordinary: a few ms behind.
        assert_eq!(unwrap_tick(1_000, 1_010), 1_000);
        // 60 days of uptime: the 32-bit counter has wrapped once.
        let now = 60 * day;
        let t32 = (now - 25) as u32;
        assert_eq!(unwrap_tick(t32, now), now - 25);
        // Event stamped just before the wrap, read just after it.
        let now = (1u64 << 32) + 5;
        assert_eq!(unwrap_tick(u32::MAX - 4, now), (1u64 << 32) - 5);
        // Event slightly ahead of the clock.
        assert_eq!(unwrap_tick(1_003, 1_000), 1_003);
        assert_eq!(unwrap_tick((now + 3) as u32, now), now + 3);
        // Right after boot nothing goes below zero.
        assert_eq!(unwrap_tick(u32::MAX, 3), 0);
        assert_eq!(unwrap_tick(10, 3), 10);
    }

    #[test]
    fn rebaser_is_monotonic_and_rejects_bogus_times() {
        let mut r = TimeRebaser::new();
        assert_eq!(r.last(), 0);
        let now = 5 * 86_400_000u64;
        assert_eq!(r.rebase((now - 30) as u32, now), now - 30);
        assert_eq!(r.rebase((now - 20) as u32, now), now - 20);
        // An earlier stamp (injected input with its own time) does not go back.
        assert_eq!(r.rebase((now - 500) as u32, now), now - 20);
        // A stamp far in the past (SendInput with time 1) becomes now.
        assert_eq!(r.rebase(1, now), now);
        // A stamp far ahead becomes now too.
        let later = now + 10;
        assert_eq!(r.rebase((later + 5_000) as u32, later), later);
        assert_eq!(r.last(), later);
        // Across the 49.7 day wrap.
        let mut r = TimeRebaser::new();
        let wrap = 1u64 << 32;
        assert_eq!(r.rebase((wrap - 2) as u32, wrap + 1), wrap - 2);
        assert_eq!(r.rebase(0, wrap + 1), wrap);
        assert_eq!(r.rebase(1, wrap + 1), wrap + 1);
        // Stamps for events the platform makes never go backwards either.
        assert_eq!(r.stamp_now(wrap + 10), wrap + 10);
        assert_eq!(r.stamp_now(wrap), wrap + 10);
        assert_eq!(r.last(), wrap + 10);
    }

    #[test]
    fn modifier_snapshot_mapping() {
        let held =
            held_modifiers(|code| code == vk::RCONTROL || code == vk::LSHIFT || code == vk::LWIN);
        assert_eq!(
            held,
            vec![
                (Mod::Ctrl, Side::Right),
                (Mod::Shift, Side::Left),
                (Mod::Win, Side::Left)
            ]
        );
        assert!(held_modifiers(|_| false).is_empty());
        assert_eq!(held_modifiers(|_| true).len(), 8);
        assert!(async_key_down(i16::MIN));
        assert!(async_key_down(-32767));
        assert!(!async_key_down(1), "the low bit is unreliable and ignored");
        assert!(!async_key_down(0));
        assert!(key_toggled(1));
        assert!(key_toggled(-127));
        assert!(!key_toggled(i16::MIN));
    }

    #[test]
    fn watchdog_predicate() {
        // The hook saw input 100 ms after the system did: alive.
        assert!(!hook_looks_dead(1_000, 1_100, WATCHDOG_THRESHOLD_MS));
        // Same tick: alive.
        assert!(!hook_looks_dead(1_000, 1_000, WATCHDOG_THRESHOLD_MS));
        // Within the threshold: alive (the pipeline may simply be behind).
        assert!(!hook_looks_dead(4_000, 1_000, WATCHDOG_THRESHOLD_MS));
        // System input 3.5 s after the hook's last event: presumed removed.
        assert!(hook_looks_dead(4_500, 1_000, WATCHDOG_THRESHOLD_MS));
        // Wrap-safe.
        assert!(hook_looks_dead(
            4_000,
            u32::MAX - 1_000,
            WATCHDOG_THRESHOLD_MS
        ));
        assert!(!hook_looks_dead(
            u32::MAX - 1_000,
            4_000,
            WATCHDOG_THRESHOLD_MS
        ));
    }

    #[test]
    fn stall_probe_reports_long_gaps_only() {
        let mut p = StallProbe::new();
        // The first tick only sets the reference.
        assert_eq!(p.tick(1_000, STALL_THRESHOLD_MS), None);
        // Healthy: the period plus timer and tick granularity.
        assert_eq!(p.tick(1_062, STALL_THRESHOLD_MS), None);
        assert_eq!(p.tick(1_140, STALL_THRESHOLD_MS), None);
        // A 400 ms stall shows as at least 400 ms.
        assert_eq!(p.tick(1_540, STALL_THRESHOLD_MS), Some(400));
        // Measured from the last tick, so the next healthy tick is quiet.
        assert_eq!(p.tick(1_590, STALL_THRESHOLD_MS), None);
        // Exactly the threshold counts; a clock that does not move does not.
        assert_eq!(p.tick(1_740, STALL_THRESHOLD_MS), Some(150));
        assert_eq!(p.tick(1_740, STALL_THRESHOLD_MS), None);
        // Smaller than one probe period plus slack: never a stall.
        const { assert!(STALL_THRESHOLD_MS > 2 * STALL_PROBE_MS as u64) };
    }

    #[test]
    fn modifier_events_pack_and_reconcile() {
        assert_eq!(unpack_modifier_event(0), None);
        for (down, t) in [(true, 0), (false, 0), (true, 123_456_789), (false, 1)] {
            assert_eq!(
                unpack_modifier_event(pack_modifier_event(down, t)),
                Some((down, t))
            );
        }
        assert_eq!(modifier_index(vk::LCONTROL), Some(0));
        assert_eq!(modifier_index(vk::RWIN), Some(7));
        assert_eq!(modifier_index(vk::F24), None);

        let ctrl_l = (Mod::Ctrl, Side::Left);
        let shift_l = (Mod::Shift, Side::Left);
        let now = 10_000;
        let mut recent = [None; 8];
        // No stream events: GetAsyncKeyState decides.
        assert_eq!(reconcile_held(&[ctrl_l], &recent, now), vec![ctrl_l]);
        // Left Ctrl was released 5 ms ago but the asynchronous state still
        // says down: the stream wins, Ctrl is not re-pressed.
        recent[0] = Some((false, now - 5));
        assert_eq!(reconcile_held(&[ctrl_l], &recent, now), vec![]);
        // Left Shift was pressed 5 ms ago but is not yet down
        // asynchronously: the stream wins again.
        recent[2] = Some((true, now - 5));
        assert_eq!(reconcile_held(&[], &recent, now), vec![shift_l]);
        // Older stream events no longer override the system.
        recent[0] = Some((false, now - RESYNC_STREAM_TRUST_MS - 1));
        recent[2] = Some((true, now - RESYNC_STREAM_TRUST_MS - 1));
        assert_eq!(reconcile_held(&[ctrl_l], &recent, now), vec![ctrl_l]);
        // A record stamped slightly ahead of the clock is recent.
        recent[0] = Some((false, now + 10));
        assert_eq!(reconcile_held(&[ctrl_l], &recent, now), vec![]);
    }

    #[test]
    fn raw_event_is_small_and_plain() {
        // Copied by value inside the hook procedure: keep it compact.
        assert!(std::mem::size_of::<RawEvent>() <= 40);
        let e = RawEvent::default();
        let copy = e;
        assert_eq!(e, copy);
        assert_eq!(high_word(0x0078_0000), 120);
        assert_eq!(high_word(0xFF88_0000) as i16, -120);
    }
}
