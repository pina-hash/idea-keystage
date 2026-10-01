//! The lock-free hand-off between the hook procedures and the pipeline
//! thread, and the receiving side that turns raw records into core events.
//!
//! The hook procedures (Windows only) write [`RawEvent`] records into
//! [`InputShared::queue`] with `force_push`, counting overwrites in
//! [`InputShared::dropped`], and unpark the consumer thread. The consumer
//! owns the one [`InputReceiver`] and calls [`InputReceiver::drain_into`]
//! whenever it wakes.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_queue::ArrayQueue;
use keystage_core::{InputEvent, Mod, PhysKey, Side};

use super::convert::{self, RawEvent, TimeRebaser};

/// Capacity of the preallocated queue: about 16 seconds of a 1000 Hz mouse
/// with the pipeline stalled.
pub const QUEUE_CAPACITY: usize = 16_384;

/// State shared by the hook thread, its procedures, the watchdog, the
/// control handle and the receiver. Only atomics and the lock-free queue.
#[derive(Debug)]
pub(crate) struct InputShared {
    pub(crate) queue: ArrayQueue<RawEvent>,
    /// Records overwritten because the queue was full.
    pub(crate) dropped: AtomicU64,
    /// Set by `pause()` before the hooks are removed, so recording stops at
    /// once; the receiver also discards anything still queued.
    pub(crate) paused: AtomicBool,
    /// Both hooks are installed right now.
    pub(crate) installed: AtomicBool,
    /// Reinstalls requested by the watchdog.
    pub(crate) reinstalls: AtomicU64,
    /// The `time` of the last record a hook delivered (or of the last
    /// install), for the watchdog.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) last_hook_tick: AtomicU32,
    /// Set after every successful install (start, resume, watchdog
    /// reinstall): input may have been missed, so the receiver adds a
    /// `ModifierResync` on its next drain (ARCHITECTURE.md section 8, rule 2).
    pub(crate) resync_pending: AtomicBool,
    /// The last event of each left/right modifier key as the receiver handed
    /// it to the consumer, indexed like [`convert::MODIFIER_VKS`] and packed
    /// with [`convert::pack_modifier_event`] (0: none). Written by the
    /// receiver only; read by every resync so a `GetAsyncKeyState` answer
    /// that lags the hook stream never overrides it.
    recent_mods: [AtomicU64; 8],
}

impl InputShared {
    pub(crate) fn new() -> InputShared {
        InputShared {
            queue: ArrayQueue::new(QUEUE_CAPACITY),
            dropped: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            installed: AtomicBool::new(false),
            reinstalls: AtomicU64::new(0),
            last_hook_tick: AtomicU32::new(0),
            resync_pending: AtomicBool::new(false),
            recent_mods: Default::default(),
        }
    }

    /// Asks the receiver to add a modifier resync on its next drain.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn request_resync(&self) {
        self.resync_pending.store(true, Ordering::Release);
    }

    /// Empties the queue (used at start and when pausing) and forgets the
    /// recent modifier events, which no longer describe a continuous stream.
    pub(crate) fn clear(&self) {
        while self.queue.pop().is_some() {}
        for slot in &self.recent_mods {
            slot.store(0, Ordering::Relaxed);
        }
    }

    /// Notes a modifier key event the receiver hands out.
    fn note_modifier(&self, code: u16, down: bool, t_ms: u64) {
        if let Some(slot) = convert::modifier_index(code).and_then(|i| self.recent_mods.get(i)) {
            slot.store(convert::pack_modifier_event(down, t_ms), Ordering::Relaxed);
        }
    }

    /// `async_held` (a `GetAsyncKeyState` read made at `now`) corrected by
    /// the hook stream: a modifier key with an event in the last
    /// [`convert::RESYNC_STREAM_TRUST_MS`] keeps the state that event gave
    /// it. Use it for every resync, so a read that lags the hook never
    /// re-presses a modifier that was just released.
    pub(crate) fn reconcile_held(&self, async_held: &[(Mod, Side)], now: u64) -> Vec<(Mod, Side)> {
        let recent: [Option<(bool, u64)>; 8] = std::array::from_fn(|i| {
            self.recent_mods
                .get(i)
                .and_then(|slot| convert::unpack_modifier_event(slot.load(Ordering::Relaxed)))
        });
        convert::reconcile_held(async_held, &recent, now)
    }

    /// What a hook procedure does with a record: overwrite the oldest record
    /// when full, never block, never allocate. Waking the consumer is the
    /// caller's job.
    #[inline]
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn record(&self, ev: RawEvent) {
        self.last_hook_tick.store(ev.time, Ordering::Relaxed);
        if self.queue.force_push(ev).is_some() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The consumer side of the input hooks. Owned by the pipeline thread.
pub struct InputReceiver {
    shared: Arc<InputShared>,
    rebaser: TimeRebaser,
    now: fn() -> u64,
    fill_scan: fn(u16) -> Option<PhysKey>,
    read_held: fn() -> Option<Vec<(Mod, Side)>>,
}

impl InputReceiver {
    pub(crate) fn new(
        shared: Arc<InputShared>,
        now: fn() -> u64,
        fill_scan: fn(u16) -> Option<PhysKey>,
        read_held: fn() -> Option<Vec<(Mod, Side)>>,
    ) -> InputReceiver {
        InputReceiver {
            shared,
            rebaser: TimeRebaser::new(),
            now,
            fill_scan,
            read_held,
        }
    }

    /// Moves every queued record into `out` as core events, in order, with
    /// timestamps rebased onto [`crate::platform::now_ms`] (monotonic).
    /// Records that carry nothing the core uses are skipped. While paused,
    /// queued records are discarded. After the hooks were (re)installed it
    /// appends an `InputEvent::ModifierResync` read with `GetAsyncKeyState`,
    /// when the desktop is readable. Returns the number of events added.
    ///
    /// Threading: call from the one consumer thread (the thread passed to
    /// `start_input`), typically after `std::thread::park` returns.
    pub fn drain_into(&mut self, out: &mut Vec<InputEvent>) -> usize {
        let now = (self.now)();
        let fill = self.fill_scan;
        let mut added = 0;
        while let Some(raw) = self.shared.queue.pop() {
            if self.shared.paused.load(Ordering::Acquire) {
                continue;
            }
            let t = self.rebaser.rebase(raw.time, now);
            if let Some(ev) = convert::to_input_event(&raw, t, &fill) {
                if let InputEvent::Key(k) = &ev {
                    self.shared.note_modifier(k.vk, k.down, k.t_ms);
                }
                out.push(ev);
                added += 1;
            }
        }
        if !self.shared.paused.load(Ordering::Acquire)
            && self.shared.resync_pending.swap(false, Ordering::AcqRel)
        {
            if let Some(held) = (self.read_held)() {
                // Read after every queued record was taken, then corrected
                // for keys the hook reported before their asynchronous state
                // was updated.
                let now = (self.now)();
                out.push(InputEvent::ModifierResync {
                    held: self.shared.reconcile_held(&held, now),
                    t_ms: self.rebaser.stamp_now(now),
                });
                added += 1;
            }
        }
        added
    }

    /// True when nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.shared.queue.is_empty()
    }

    /// Records lost to a full queue since start (should stay 0).
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// The last timestamp handed out, on the [`crate::platform::now_ms`]
    /// clock (0 before the first event).
    pub fn last_t_ms(&self) -> u64 {
        self.rebaser.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::convert::{flags, msg};
    use keystage_core::MouseButton;

    const NOW: u64 = 10_000_000;

    fn now() -> u64 {
        NOW
    }

    fn fill(code: u16) -> Option<PhysKey> {
        (code == 0x41).then_some(PhysKey::new(0x1E, false))
    }

    fn held() -> Option<Vec<(Mod, Side)>> {
        Some(vec![(Mod::Ctrl, Side::Left)])
    }

    fn unreadable() -> Option<Vec<(Mod, Side)>> {
        None
    }

    fn receiver() -> (Arc<InputShared>, InputReceiver) {
        let shared = Arc::new(InputShared::new());
        let rx = InputReceiver::new(shared.clone(), now, fill, held);
        (shared, rx)
    }

    #[test]
    fn drains_in_order_with_rebased_times() {
        let (shared, mut rx) = receiver();
        assert!(rx.is_empty());
        let t = NOW as u32;
        shared.record(RawEvent::key(msg::WM_KEYDOWN, 0x41, 0, 0, t - 20));
        shared.record(RawEvent::key(
            msg::WM_KEYUP,
            0x41,
            0x1E,
            flags::LLKHF_UP,
            t - 10,
        ));
        shared.record(RawEvent::mouse(msg::WM_LBUTTONDOWN, 5, 6, 0, 0, t - 5));
        shared.record(RawEvent::mouse(0x00A0, 5, 6, 0, 0, t - 4)); // skipped
        assert!(!rx.is_empty());
        let mut out = Vec::new();
        assert_eq!(rx.drain_into(&mut out), 3);
        assert!(rx.is_empty());
        match &out[0] {
            InputEvent::Key(k) => {
                assert!(k.down);
                assert_eq!(k.phys, PhysKey::new(0x1E, false), "scan filled");
                assert_eq!(k.t_ms, NOW - 20);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&out[1], InputEvent::Key(k) if !k.down && k.t_ms == NOW - 10));
        assert!(matches!(
            &out[2],
            InputEvent::Button { button: MouseButton::Left, down: true, x: 5, y: 6, t_ms, .. }
                if *t_ms == NOW - 5
        ));
        assert_eq!(rx.last_t_ms(), NOW - 4);
        assert_eq!(rx.drain_into(&mut out), 0);
    }

    #[test]
    fn overflow_overwrites_oldest_and_counts() {
        let (shared, mut rx) = receiver();
        let extra = 10;
        for i in 0..(QUEUE_CAPACITY + extra) {
            shared.record(RawEvent::mouse(
                msg::WM_MOUSEMOVE,
                i as i32,
                0,
                0,
                0,
                NOW as u32,
            ));
        }
        assert_eq!(rx.dropped(), extra as u64);
        let mut out = Vec::with_capacity(QUEUE_CAPACITY);
        assert_eq!(rx.drain_into(&mut out), QUEUE_CAPACITY);
        // The oldest records were the ones overwritten.
        assert!(matches!(out[0], InputEvent::Move { x, .. } if x == extra as i32));
    }

    #[test]
    fn paused_records_are_discarded() {
        let (shared, mut rx) = receiver();
        shared.record(RawEvent::key(msg::WM_KEYDOWN, 0x41, 0x1E, 0, NOW as u32));
        shared.paused.store(true, Ordering::Release);
        let mut out = Vec::new();
        assert_eq!(rx.drain_into(&mut out), 0);
        assert!(out.is_empty());
        assert!(rx.is_empty(), "discarded, not kept for later");
        shared.paused.store(false, Ordering::Release);
        shared.record(RawEvent::key(msg::WM_KEYDOWN, 0x41, 0x1E, 0, NOW as u32));
        assert_eq!(rx.drain_into(&mut out), 1);
    }

    #[test]
    fn a_reinstall_adds_one_modifier_resync_after_the_queued_events() {
        let (shared, mut rx) = receiver();
        shared.record(RawEvent::key(
            msg::WM_KEYDOWN,
            0x41,
            0x1E,
            0,
            NOW as u32 - 3,
        ));
        shared.request_resync();
        let mut out = Vec::new();
        assert_eq!(rx.drain_into(&mut out), 2);
        assert!(matches!(out[0], InputEvent::Key(_)));
        assert_eq!(
            out[1],
            InputEvent::ModifierResync {
                held: vec![(Mod::Ctrl, Side::Left)],
                t_ms: NOW
            }
        );
        // Only once.
        out.clear();
        assert_eq!(rx.drain_into(&mut out), 0);
        // Not while paused; the request is kept for after the pause.
        shared.request_resync();
        shared.paused.store(true, Ordering::Release);
        assert_eq!(rx.drain_into(&mut out), 0);
        shared.paused.store(false, Ordering::Release);
        assert_eq!(rx.drain_into(&mut out), 1);
        // An unreadable desktop gives no resync, and the request is spent.
        let shared = Arc::new(InputShared::new());
        let mut rx = InputReceiver::new(shared.clone(), now, fill, unreadable);
        shared.request_resync();
        assert_eq!(rx.drain_into(&mut out), 0);
        assert!(!shared.resync_pending.load(Ordering::Acquire));
    }

    #[test]
    fn a_resync_never_undoes_a_modifier_event_the_lagging_read_missed() {
        use keystage_core::keys::vk;
        let (shared, mut rx) = receiver();
        // Left Ctrl was released; the record is taken, but GetAsyncKeyState
        // (`held`) still reports it down, as it does before Windows updates
        // the asynchronous state after the hook returns.
        shared.record(RawEvent::key(
            msg::WM_KEYUP,
            u32::from(vk::LCONTROL),
            0x1D,
            flags::LLKHF_UP,
            NOW as u32 - 2,
        ));
        shared.request_resync();
        let mut out = Vec::new();
        assert_eq!(rx.drain_into(&mut out), 2);
        assert!(matches!(out[0], InputEvent::Key(k) if !k.down));
        assert_eq!(
            out[1],
            InputEvent::ModifierResync {
                held: vec![],
                t_ms: NOW
            },
            "Ctrl stays released"
        );
        // The same correction serves resyncs read elsewhere.
        assert_eq!(
            shared.reconcile_held(&[(Mod::Ctrl, Side::Left), (Mod::Shift, Side::Right)], NOW),
            vec![(Mod::Shift, Side::Right)]
        );
        // Long after the event, the system's answer stands again.
        assert_eq!(
            shared.reconcile_held(&[(Mod::Ctrl, Side::Left)], NOW + 1_000),
            vec![(Mod::Ctrl, Side::Left)]
        );
        // Pausing (clear) forgets the stream's view.
        shared.clear();
        assert_eq!(
            shared.reconcile_held(&[(Mod::Ctrl, Side::Left)], NOW),
            vec![(Mod::Ctrl, Side::Left)]
        );
    }

    #[test]
    fn record_tracks_the_last_hook_tick_and_clear_empties() {
        let shared = InputShared::new();
        shared.record(RawEvent::key(msg::WM_KEYDOWN, 0x41, 0x1E, 0, 1234));
        assert_eq!(shared.last_hook_tick.load(Ordering::Relaxed), 1234);
        shared.clear();
        assert!(shared.queue.is_empty());
        assert_eq!(shared.queue.capacity(), QUEUE_CAPACITY);
    }
}
