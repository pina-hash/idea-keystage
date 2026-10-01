//! Recording clock: maps the app's monotonic milliseconds (the same clock as
//! `Callout::t_ms`) to milliseconds into the recorded video.
//!
//! Inputs, both stored in the session file:
//!
//! * Record state changes from OBS (`RecordStateChanged`: started, paused,
//!   resumed, stopped), each stamped with the monotonic time it arrived.
//! * Duration samples from OBS `GetRecordStatus` (`outputDuration` in
//!   milliseconds, `outputPaused`), polled about once a second while OBS
//!   records, each stamped with the monotonic time it describes.
//!
//! A take is one recording from start to stop. Inside a take, running spans
//! advance one to one with the monotonic clock and paused spans hold the
//! video time still. Duration samples anchor the running spans: between two
//! samples the mapping is interpolated, after the last one it advances one
//! to one. Without OBS data the mapping falls back to the session's own wall
//! time (milliseconds since the session started).

use serde::{Deserialize, Serialize};

/// A record state change reported by OBS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordEvent {
    Started,
    Paused,
    Resumed,
    Stopped,
}

/// A record state change at a monotonic time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockEvent {
    pub t_ms: u64,
    pub event: RecordEvent,
}

/// An OBS duration sample: at monotonic time `t_ms` the recording was
/// `duration_ms` long.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSample {
    pub t_ms: u64,
    pub duration_ms: u64,
    pub paused: bool,
}

/// What the session stored about the recording.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordingLog {
    pub events: Vec<ClockEvent>,
    pub samples: Vec<ClockSample>,
}

impl RecordingLog {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.samples.is_empty()
    }
}

/// Where a monotonic instant falls relative to the chosen take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Position {
    /// Before the take started (or before the session, without OBS).
    Before,
    /// While the take was recording: the instant is in the video.
    During,
    /// While the take was paused: not in the video; `rec_ms` is the point
    /// where the video resumes.
    Paused,
    /// After the take stopped; `rec_ms` is the end of the video.
    After,
}

/// A mapped instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mapped {
    /// Milliseconds into the video, clamped into it.
    pub rec_ms: u64,
    pub position: Position,
}

impl Mapped {
    pub fn in_video(&self) -> bool {
        self.position == Position::During
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Span {
    from: u64,
    /// Exclusive end; `None` while open.
    to: Option<u64>,
    running: bool,
    /// Video time at `from` (for a paused span, the frozen time).
    base_rec: u64,
    /// `(t_ms, duration_ms)` samples inside a running span, in time order.
    anchors: Vec<(u64, u64)>,
}

impl Span {
    fn rec_at(&self, t: u64) -> u64 {
        if !self.running {
            return self.base_rec;
        }
        let a = &self.anchors;
        if a.is_empty() {
            return self.base_rec + t.saturating_sub(self.from);
        }
        let first = a[0];
        if t <= first.0 {
            return first.1.saturating_sub(first.0 - t);
        }
        // Last anchor at or before t.
        let i = a.partition_point(|&(at, _)| at <= t) - 1;
        let (t0, d0) = a[i];
        match a.get(i + 1) {
            Some(&(t1, d1)) if t1 > t0 && d1 >= d0 => {
                let num = u128::from(t - t0) * u128::from(d1 - d0);
                d0 + (num / u128::from(t1 - t0)) as u64
            }
            _ => d0 + (t - t0),
        }
    }

    fn contains(&self, t: u64) -> bool {
        t >= self.from && self.to.is_none_or(|to| t < to)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Take {
    start: u64,
    end: Option<u64>,
    spans: Vec<Span>,
}

impl Take {
    fn last_span_mut(&mut self) -> &mut Span {
        // A take is always created with one span.
        let n = self.spans.len() - 1;
        &mut self.spans[n]
    }

    fn rec_at_end(&self, t: u64) -> u64 {
        match self.spans.last() {
            Some(s) => s.rec_at(t),
            None => 0,
        }
    }

    fn close(&mut self, t: u64) {
        let span = self.last_span_mut();
        if span.to.is_none() {
            span.to = Some(t.max(span.from));
        }
        self.end = Some(t.max(self.start));
    }
}

enum Item<'a> {
    Event(&'a ClockEvent),
    Sample(&'a ClockSample),
}

impl Item<'_> {
    fn t(&self) -> u64 {
        match self {
            Item::Event(e) => e.t_ms,
            Item::Sample(s) => s.t_ms,
        }
    }
}

/// The mapping for one session.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordingClock {
    session_start: u64,
    session_end: Option<u64>,
    takes: Vec<Take>,
    selected: usize,
}

impl RecordingClock {
    /// Builds the mapping. `session_end` is the stop time, `None` while the
    /// session is still running. The last take is selected.
    pub fn new(log: &RecordingLog, session_start: u64, session_end: Option<u64>) -> Self {
        let mut items: Vec<Item<'_>> = log
            .events
            .iter()
            .map(Item::Event)
            .chain(log.samples.iter().map(Item::Sample))
            .collect();
        // Stable: at equal times events come before samples, in stored order.
        items.sort_by_key(|i| (i.t(), matches!(i, Item::Sample(_))));

        let mut takes: Vec<Take> = Vec::new();
        let mut cur: Option<Take> = None;
        for item in items {
            match item {
                Item::Event(e) => {
                    let t = e.t_ms;
                    match e.event {
                        RecordEvent::Started => {
                            if let Some(mut open) = cur.take() {
                                open.close(t);
                                takes.push(open);
                            }
                            cur = Some(Take {
                                start: t,
                                end: None,
                                spans: vec![Span {
                                    from: t,
                                    to: None,
                                    running: true,
                                    base_rec: 0,
                                    anchors: Vec::new(),
                                }],
                            });
                        }
                        RecordEvent::Paused => {
                            if let Some(take) = cur.as_mut() {
                                let span = take.last_span_mut();
                                if span.running {
                                    let frozen = span.rec_at(t);
                                    span.to = Some(t.max(span.from));
                                    take.spans.push(Span {
                                        from: t,
                                        to: None,
                                        running: false,
                                        base_rec: frozen,
                                        anchors: Vec::new(),
                                    });
                                }
                            }
                        }
                        RecordEvent::Resumed => match cur.as_mut() {
                            Some(take) => {
                                let span = take.last_span_mut();
                                if !span.running {
                                    let frozen = span.base_rec;
                                    span.to = Some(t.max(span.from));
                                    take.spans.push(Span {
                                        from: t,
                                        to: None,
                                        running: true,
                                        base_rec: frozen,
                                        anchors: Vec::new(),
                                    });
                                }
                            }
                            None => {
                                // The recording was paused before Keystage
                                // knew about it. Its video time is unknown
                                // until a sample arrives.
                                cur = Some(Take {
                                    start: t,
                                    end: None,
                                    spans: vec![Span {
                                        from: t,
                                        to: None,
                                        running: true,
                                        base_rec: 0,
                                        anchors: Vec::new(),
                                    }],
                                });
                            }
                        },
                        RecordEvent::Stopped => {
                            if let Some(mut take) = cur.take() {
                                take.close(t);
                                takes.push(take);
                            }
                        }
                    }
                }
                Item::Sample(s) => {
                    // OBS reports 0 while the output is not active.
                    if s.duration_ms == 0 {
                        continue;
                    }
                    match cur.as_mut() {
                        None => {
                            // OBS was already recording when the session
                            // learned of it: an implicit take.
                            let from = s.t_ms.saturating_sub(s.duration_ms);
                            cur = Some(Take {
                                start: if s.paused { s.t_ms } else { from },
                                end: None,
                                spans: vec![Span {
                                    from: if s.paused { s.t_ms } else { from },
                                    to: None,
                                    running: !s.paused,
                                    base_rec: if s.paused { s.duration_ms } else { 0 },
                                    anchors: if s.paused {
                                        Vec::new()
                                    } else {
                                        vec![(s.t_ms, s.duration_ms)]
                                    },
                                }],
                            });
                        }
                        Some(take) => {
                            let span = take.last_span_mut();
                            match (span.running, s.paused) {
                                (true, false) => {
                                    let keep = span
                                        .anchors
                                        .last()
                                        .is_none_or(|&(at, d)| s.t_ms > at && s.duration_ms >= d);
                                    if keep {
                                        span.anchors.push((s.t_ms, s.duration_ms));
                                    }
                                }
                                (false, true) => span.base_rec = s.duration_ms,
                                // An event and a poll crossed; the event wins.
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        if let Some(open) = cur {
            takes.push(open);
        }
        let selected = takes.len().saturating_sub(1);
        RecordingClock {
            session_start,
            session_end,
            takes,
            selected,
        }
    }

    /// Number of takes (recordings) the session saw. 0 means no OBS data:
    /// the mapping uses session wall time.
    pub fn takes(&self) -> usize {
        self.takes.len()
    }

    /// True when OBS timing is used.
    pub fn uses_obs(&self) -> bool {
        !self.takes.is_empty()
    }

    /// The selected take (0-based).
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Selects a take by 0-based index. Out of range keeps the current one
    /// and returns false.
    pub fn select(&mut self, take: usize) -> bool {
        if take < self.takes.len() {
            self.selected = take;
            true
        } else {
            false
        }
    }

    /// End of the take before the selected one, when there is one. Instants
    /// before it belong to an earlier recording, not to the selected video's
    /// lead-in.
    pub(crate) fn previous_take_end(&self) -> Option<u64> {
        let prev = self.takes.get(self.selected.checked_sub(1)?)?;
        // Every take but the last is closed (a new start closes the open one).
        Some(prev.end.unwrap_or(prev.start))
    }

    /// Length of the video in milliseconds, when known: the selected take's
    /// length (up to the session end for a take still open), or the session
    /// length without OBS.
    pub fn video_length_ms(&self) -> Option<u64> {
        match self.takes.get(self.selected) {
            Some(take) => {
                let end = take.end.or(self.session_end)?;
                Some(take.rec_at_end(end))
            }
            None => self
                .session_end
                .map(|e| e.saturating_sub(self.session_start)),
        }
    }

    /// Maps a monotonic instant to video time in the selected take.
    pub fn map(&self, t: u64) -> Mapped {
        let Some(take) = self.takes.get(self.selected) else {
            if t < self.session_start {
                return Mapped {
                    rec_ms: 0,
                    position: Position::Before,
                };
            }
            if let Some(end) = self.session_end {
                if t > end {
                    return Mapped {
                        rec_ms: end - self.session_start,
                        position: Position::After,
                    };
                }
            }
            return Mapped {
                rec_ms: t - self.session_start,
                position: Position::During,
            };
        };
        if t < take.start {
            return Mapped {
                rec_ms: 0,
                position: Position::Before,
            };
        }
        if let Some(end) = take.end {
            if t >= end {
                return Mapped {
                    rec_ms: take.rec_at_end(end),
                    position: Position::After,
                };
            }
        }
        let span = take
            .spans
            .iter()
            .rev()
            .find(|s| s.contains(t))
            .or(take.spans.last());
        match span {
            Some(s) if s.running => Mapped {
                rec_ms: s.rec_at(t),
                position: Position::During,
            },
            Some(s) => Mapped {
                rec_ms: s.base_rec,
                position: Position::Paused,
            },
            None => Mapped {
                rec_ms: 0,
                position: Position::Before,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t_ms: u64, event: RecordEvent) -> ClockEvent {
        ClockEvent { t_ms, event }
    }

    fn sample(t_ms: u64, duration_ms: u64, paused: bool) -> ClockSample {
        ClockSample {
            t_ms,
            duration_ms,
            paused,
        }
    }

    #[test]
    fn falls_back_to_session_time() {
        let c = RecordingClock::new(&RecordingLog::default(), 1_000, Some(61_000));
        assert!(!c.uses_obs());
        assert_eq!(c.takes(), 0);
        assert_eq!(c.map(1_000).rec_ms, 0);
        assert_eq!(c.map(31_000).rec_ms, 30_000);
        assert!(c.map(31_000).in_video());
        assert_eq!(c.map(500).position, Position::Before);
        assert_eq!(c.map(70_000).position, Position::After);
        assert_eq!(c.map(70_000).rec_ms, 60_000);
        assert_eq!(c.video_length_ms(), Some(60_000));
        let open = RecordingClock::new(&RecordingLog::default(), 1_000, None);
        assert_eq!(open.video_length_ms(), None);
        assert!(open.map(10_000_000).in_video());
    }

    #[test]
    fn events_only_with_a_pause() {
        let log = RecordingLog {
            events: vec![
                ev(10_000, RecordEvent::Started),
                ev(20_000, RecordEvent::Paused),
                ev(25_000, RecordEvent::Resumed),
                ev(40_000, RecordEvent::Stopped),
            ],
            samples: vec![],
        };
        let c = RecordingClock::new(&log, 0, Some(50_000));
        assert_eq!(c.takes(), 1);
        assert_eq!(c.map(5_000).position, Position::Before);
        assert_eq!(c.map(10_000).rec_ms, 0);
        assert_eq!(c.map(15_000).rec_ms, 5_000);
        let p = c.map(22_000);
        assert_eq!((p.rec_ms, p.position), (10_000, Position::Paused));
        assert_eq!(c.map(25_000).rec_ms, 10_000);
        assert_eq!(c.map(30_000).rec_ms, 15_000);
        let after = c.map(45_000);
        assert_eq!((after.rec_ms, after.position), (25_000, Position::After));
        assert_eq!(c.video_length_ms(), Some(25_000));
    }

    #[test]
    fn samples_correct_event_latency_and_drift() {
        // The started event arrived 300 ms after OBS began counting frames,
        // and media time runs 1 percent slower than the monotonic clock.
        let log = RecordingLog {
            events: vec![ev(10_000, RecordEvent::Started)],
            samples: vec![
                sample(11_000, 1_300, false),
                sample(21_000, 11_200, false),
                sample(31_000, 21_100, false),
            ],
        };
        let c = RecordingClock::new(&log, 0, None);
        assert_eq!(
            c.map(10_000).rec_ms,
            300,
            "extrapolated back from the first sample"
        );
        assert_eq!(c.map(11_000).rec_ms, 1_300);
        assert_eq!(c.map(16_000).rec_ms, 6_250, "interpolated");
        assert_eq!(c.map(21_000).rec_ms, 11_200);
        assert_eq!(
            c.map(41_000).rec_ms,
            31_100,
            "one to one after the last sample"
        );
        assert_eq!(c.video_length_ms(), None, "still recording, session open");
    }

    #[test]
    fn paused_sample_sets_the_frozen_time() {
        let log = RecordingLog {
            events: vec![
                ev(0, RecordEvent::Started),
                ev(10_000, RecordEvent::Paused),
                ev(20_000, RecordEvent::Resumed),
            ],
            samples: vec![sample(15_000, 9_950, true), sample(21_000, 10_950, false)],
        };
        let c = RecordingClock::new(&log, 0, None);
        assert_eq!(c.map(12_000).rec_ms, 9_950);
        assert_eq!(c.map(12_000).position, Position::Paused);
        assert_eq!(c.map(21_000).rec_ms, 10_950);
        assert_eq!(c.map(20_000).rec_ms, 9_950);
    }

    #[test]
    fn implicit_take_when_obs_was_already_recording() {
        // Connected at t=100_000 while OBS had recorded 60 s.
        let log = RecordingLog {
            events: vec![ev(160_000, RecordEvent::Stopped)],
            samples: vec![sample(100_000, 60_000, false)],
        };
        let c = RecordingClock::new(&log, 90_000, Some(170_000));
        assert_eq!(c.takes(), 1);
        assert_eq!(c.map(100_000).rec_ms, 60_000);
        assert_eq!(c.map(95_000).rec_ms, 55_000);
        assert!(c.map(95_000).in_video());
        assert_eq!(c.map(130_000).rec_ms, 90_000);
        assert_eq!(c.map(165_000).position, Position::After);
        assert_eq!(c.video_length_ms(), Some(120_000));
    }

    #[test]
    fn implicit_paused_take() {
        let log = RecordingLog {
            events: vec![ev(50_000, RecordEvent::Resumed)],
            samples: vec![sample(40_000, 30_000, true)],
        };
        let c = RecordingClock::new(&log, 0, None);
        assert_eq!(c.map(45_000).position, Position::Paused);
        assert_eq!(c.map(45_000).rec_ms, 30_000);
        assert_eq!(c.map(55_000).rec_ms, 35_000);
        assert_eq!(c.map(10_000).position, Position::Before);
    }

    #[test]
    fn resumed_without_start_is_an_implicit_take() {
        let log = RecordingLog {
            events: vec![ev(5_000, RecordEvent::Resumed)],
            samples: vec![sample(6_000, 41_000, false)],
        };
        let c = RecordingClock::new(&log, 0, None);
        assert_eq!(c.map(6_000).rec_ms, 41_000);
        assert_eq!(c.map(5_000).rec_ms, 40_000);
    }

    #[test]
    fn several_takes_last_selected() {
        let log = RecordingLog {
            events: vec![
                ev(1_000, RecordEvent::Started),
                ev(5_000, RecordEvent::Stopped),
                ev(10_000, RecordEvent::Started),
                ev(30_000, RecordEvent::Stopped),
            ],
            samples: vec![],
        };
        let mut c = RecordingClock::new(&log, 0, Some(40_000));
        assert_eq!(c.takes(), 2);
        assert_eq!(c.selected(), 1);
        assert_eq!(c.map(3_000).position, Position::Before);
        assert_eq!(c.map(12_000).rec_ms, 2_000);
        assert!(c.select(0));
        assert_eq!(c.map(3_000).rec_ms, 2_000);
        assert_eq!(c.map(12_000).position, Position::After);
        assert_eq!(c.video_length_ms(), Some(4_000));
        assert!(!c.select(2));
        assert_eq!(c.selected(), 0);
        assert_eq!(c.previous_take_end(), None);
        assert!(c.select(1));
        assert_eq!(c.previous_take_end(), Some(5_000));
        let none = RecordingClock::new(&RecordingLog::default(), 0, Some(40_000));
        assert_eq!(none.previous_take_end(), None);
    }

    #[test]
    fn started_twice_closes_the_first_take() {
        let log = RecordingLog {
            events: vec![
                ev(1_000, RecordEvent::Started),
                ev(5_000, RecordEvent::Started),
            ],
            samples: vec![],
        };
        let c = RecordingClock::new(&log, 0, None);
        assert_eq!(c.takes(), 2);
        assert_eq!(c.map(6_000).rec_ms, 1_000);
        assert_eq!(c.previous_take_end(), Some(5_000));
    }

    #[test]
    fn crossed_or_bad_samples_are_ignored() {
        let log = RecordingLog {
            events: vec![ev(0, RecordEvent::Started), ev(10_000, RecordEvent::Paused)],
            samples: vec![
                sample(500, 0, false),      // output not active yet
                sample(5_000, 5_000, true), // poll crossed the pause event
                sample(6_000, 6_000, false),
                sample(6_000, 6_500, false),  // same instant again
                sample(7_000, 5_000, false),  // went backwards
                sample(12_000, 9_000, false), // running sample in a paused span
            ],
        };
        let c = RecordingClock::new(&log, 0, None);
        assert_eq!(c.map(500).rec_ms, 500);
        assert_eq!(c.map(8_000).rec_ms, 8_000);
        assert_eq!(c.map(12_000).rec_ms, 10_000);
        assert_eq!(c.map(12_000).position, Position::Paused);
    }

    #[test]
    fn serde_shape() {
        let log = RecordingLog {
            events: vec![ev(1, RecordEvent::Started)],
            samples: vec![sample(2, 3, false)],
        };
        let json = serde_json::to_string(&log).unwrap();
        assert_eq!(
            json,
            r#"{"events":[{"t_ms":1,"event":"started"}],"samples":[{"t_ms":2,"duration_ms":3,"paused":false}]}"#
        );
        let back: RecordingLog = serde_json::from_str(&json).unwrap();
        assert_eq!(back, log);
        let empty: RecordingLog = serde_json::from_str("{}").unwrap();
        assert!(empty.is_empty());
    }
}
