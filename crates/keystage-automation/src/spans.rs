//! Dirty spans: stretches of a take during which another window covered
//! the capture monitor, so a builder can cut them (docs/ARCHITECTURE.md
//! section 14). `perform` checks the occluders before it starts and at
//! every marker; a span opens at the first check that finds one and closes
//! at the next check that finds none, or at the end of the run.

use serde::{Deserialize, Serialize};

use crate::occlude::Occluder;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtySpan {
    /// Session monotonic milliseconds.
    pub start_ms: u64,
    pub end_ms: u64,
    /// Milliseconds into the video, when OBS is linked and recording.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_end_ms: Option<u64>,
    /// The processes whose windows covered the monitor.
    pub processes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DirtySpans {
    open: Option<(u64, Vec<String>)>,
    done: Vec<DirtySpan>,
}

impl DirtySpans {
    /// One check at `t_ms`.
    pub fn observe(&mut self, t_ms: u64, found: &[Occluder]) {
        match (&mut self.open, found.is_empty()) {
            (None, false) => {
                let mut p: Vec<String> = found.iter().map(|o| o.process.clone()).collect();
                p.sort();
                p.dedup();
                self.open = Some((t_ms, p));
            }
            (Some((_, procs)), false) => {
                for o in found {
                    if !procs.contains(&o.process) {
                        procs.push(o.process.clone());
                    }
                }
                procs.sort();
            }
            (Some(_), true) => {
                if let Some((start, processes)) = self.open.take() {
                    self.done.push(DirtySpan {
                        start_ms: start,
                        end_ms: t_ms.max(start),
                        video_start_ms: None,
                        video_end_ms: None,
                        processes,
                    });
                }
            }
            (None, true) => {}
        }
    }

    /// Closes an open span at `t_ms` and returns every span, with video
    /// times from `video` (session ms to video ms) where it has one.
    pub fn finish(mut self, t_ms: u64, video: &dyn Fn(u64) -> Option<u64>) -> Vec<DirtySpan> {
        self.observe(t_ms, &[]);
        for s in &mut self.done {
            s.video_start_ms = video(s.start_ms);
            s.video_end_ms = video(s.end_ms);
        }
        self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::Rect;

    fn o(p: &str) -> Occluder {
        Occluder {
            process: p.into(),
            z: 1,
            rect: Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            topmost: true,
        }
    }

    #[test]
    fn spans_open_at_the_first_dirty_check_and_close_at_the_next_clean_one() {
        let mut d = DirtySpans::default();
        d.observe(1_000, &[]);
        d.observe(2_000, &[o("claude.exe")]);
        d.observe(3_000, &[o("claude.exe"), o("teams.exe")]);
        d.observe(4_000, &[]);
        d.observe(5_000, &[o("teams.exe")]);
        let v = d.finish(6_000, &|t| t.checked_sub(1_500));
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].start_ms, v[0].end_ms), (2_000, 4_000));
        assert_eq!(
            v[0].processes,
            vec!["claude.exe".to_string(), "teams.exe".to_string()]
        );
        assert_eq!(
            (v[0].video_start_ms, v[0].video_end_ms),
            (Some(500), Some(2_500))
        );
        assert_eq!(
            (v[1].start_ms, v[1].end_ms),
            (5_000, 6_000),
            "closed at the end"
        );
        let none = DirtySpans::default().finish(9, &|_| None);
        assert!(none.is_empty());
        let mut d = DirtySpans::default();
        d.observe(10, &[o("a.exe")]);
        let v = d.finish(10, &|_| None);
        assert_eq!(
            (v[0].start_ms, v[0].end_ms, v[0].video_start_ms),
            (10, 10, None)
        );
    }
}
