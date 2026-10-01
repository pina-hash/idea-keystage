//! What the client reports: connection state, recording state, chapter
//! support, and the outcome of a chapter request. Every message here is UI
//! text (American spelling, no em dashes); none of it goes to the log with
//! user content in it.

use std::time::Instant;

use keystage_core::ipc::{ObsState, ObsStatus as IpcObsStatus};
use keystage_core::protocol::RecordingState;

/// The connection to obs-websocket.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Connection {
    /// The client is not running.
    #[default]
    Off,
    /// Connecting, or waiting to retry.
    Connecting,
    /// Identified with OBS.
    Connected {
        obs_version: String,
        ws_version: String,
    },
    /// OBS refused the password (close code 4009) or asks for one that was
    /// not given. No retry until the password changes.
    AuthFailed { missing_password: bool },
    /// OBS's session list "Kick" button (close code 4011). No automatic
    /// reconnect; [`crate::ObsClient::reconnect`] or a new config retries.
    Kicked,
    /// OBS cannot speak obs-websocket RPC version 1 (close code 4010, or an
    /// older `rpcVersion` in its Hello). No automatic retry.
    Unsupported { reason: String },
    /// Anything else; the client retries with backoff.
    Error(String),
}

impl Connection {
    pub fn is_connected(&self) -> bool {
        matches!(self, Connection::Connected { .. })
    }

    /// The `Status.obs.state` value.
    pub fn ipc_state(&self) -> ObsState {
        match self {
            Connection::Off => ObsState::Off,
            Connection::Connecting => ObsState::Connecting,
            Connection::Connected { .. } => ObsState::Connected,
            Connection::AuthFailed { .. } => ObsState::AuthFailed,
            Connection::Kicked | Connection::Unsupported { .. } | Connection::Error(_) => {
                ObsState::Error
            }
        }
    }

    /// What to tell the user, when there is something to tell.
    pub fn message(&self) -> Option<String> {
        match self {
            Connection::Off | Connection::Connecting | Connection::Connected { .. } => None,
            Connection::AuthFailed {
                missing_password: true,
            } => Some(
                "OBS asks for a password. Copy it from Tools > WebSocket Server Settings > Show Connect Info in OBS.".into(),
            ),
            Connection::AuthFailed {
                missing_password: false,
            } => Some(
                "OBS did not accept the password. Check it in Tools > WebSocket Server Settings > Show Connect Info in OBS.".into(),
            ),
            Connection::Kicked => Some(
                "OBS disconnected Keystage from its WebSocket session list. Reconnect from settings when ready.".into(),
            ),
            Connection::Unsupported { reason } => Some(reason.clone()),
            Connection::Error(m) => Some(m.clone()),
        }
    }
}

/// The recording output's state, from `RecordStateChanged.outputState`
/// (never from `outputActive`, which is false while paused).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RecordState {
    #[default]
    Stopped,
    Starting,
    Recording,
    Paused,
    Stopping,
}

impl RecordState {
    /// Parses an `ObsOutputState` string. Unknown and reconnect states
    /// (which recording never reports) give `None`.
    pub fn from_output_state(s: &str) -> Option<RecordState> {
        Some(match s {
            "OBS_WEBSOCKET_OUTPUT_STARTING" => RecordState::Starting,
            "OBS_WEBSOCKET_OUTPUT_STARTED" | "OBS_WEBSOCKET_OUTPUT_RESUMED" => {
                RecordState::Recording
            }
            "OBS_WEBSOCKET_OUTPUT_PAUSED" => RecordState::Paused,
            "OBS_WEBSOCKET_OUTPUT_STOPPING" => RecordState::Stopping,
            "OBS_WEBSOCKET_OUTPUT_STOPPED" => RecordState::Stopped,
            _ => return None,
        })
    }

    /// True while a file is being written (paused included).
    pub fn active(self) -> bool {
        matches!(
            self,
            RecordState::Recording | RecordState::Paused | RecordState::Stopping
        )
    }
}

/// One `GetRecordStatus` answer: at monotonic instant `at` (the midpoint
/// between sending the request and reading the answer) the recording was
/// `duration_ms` long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DurationSample {
    pub at: Instant,
    pub duration_ms: u64,
    pub paused: bool,
}

/// Recording state while connected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recording {
    pub state: RecordState,
    /// `HH:MM:SS.mmm` from the latest poll.
    pub timecode: String,
    pub duration_ms: u64,
    /// The latest duration sample.
    pub sample: Option<DurationSample>,
}

/// Formats milliseconds as obs-websocket does: `HH:MM:SS.mmm`.
pub fn timecode(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = ms % 3_600_000 / 60_000;
    let s = ms % 60_000 / 1000;
    let f = ms % 1000;
    format!("{h:02}:{m:02}:{s:02}.{f:03}")
}

impl Recording {
    pub fn active(&self) -> bool {
        self.state.active()
    }

    pub fn paused(&self) -> bool {
        self.state == RecordState::Paused
    }

    /// The recording time at `now`, extrapolated from the latest sample
    /// while recording (for a smoothly running clock between polls).
    pub fn duration_at(&self, now: Instant) -> u64 {
        match (self.state, self.sample) {
            (RecordState::Recording, Some(s)) if !s.paused => {
                s.duration_ms + now.saturating_duration_since(s.at).as_millis() as u64
            }
            _ => self.duration_ms,
        }
    }

    /// `AppState.recording` / `Status.obs.recording`.
    pub fn to_wire(&self) -> RecordingState {
        RecordingState {
            active: self.active(),
            paused: self.paused(),
            timecode: if self.timecode.is_empty() {
                timecode(self.duration_ms)
            } else {
                self.timecode.clone()
            },
        }
    }
}

/// Whether OBS can put chapter markers into the recording.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ChapterSupport {
    /// Not known yet (no `GetVersion` answer).
    #[default]
    Unknown,
    /// `CreateRecordChapter` is available.
    Available,
    /// This OBS has no `CreateRecordChapter` (obs-websocket before 5.5.0,
    /// OBS before 30.2).
    NotInThisObs,
    /// The current recording's format does not support chapters (OBS
    /// answered 702). Reset when the next recording starts.
    FormatUnsupported,
}

/// Everything the client knows, published on a watch channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObsStatus {
    pub connection: Connection,
    /// `None` while not connected.
    pub recording: Option<Recording>,
    pub chapters: ChapterSupport,
}

impl ObsStatus {
    /// The settings window's `Status.obs`.
    pub fn to_ipc(&self, enabled: bool) -> IpcObsStatus {
        IpcObsStatus {
            enabled,
            state: if enabled {
                self.connection.ipc_state()
            } else {
                ObsState::Off
            },
            error: if enabled {
                self.connection.message()
            } else {
                None
            },
            recording: if enabled && self.connection.is_connected() {
                self.recording.as_ref().map(Recording::to_wire)
            } else {
                None
            },
        }
    }
}

/// What happened to a chapter marker request. The marker is always in the
/// Session Log; this says whether OBS has it too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChapterOutcome {
    Created,
    NotConnected,
    NotInThisObs,
    NotRecording,
    Paused,
    /// OBS answered 702: the recording format has no chapters.
    FormatUnsupported,
    Failed {
        code: u32,
    },
    Timeout,
}

impl ChapterOutcome {
    pub fn is_created(&self) -> bool {
        *self == ChapterOutcome::Created
    }

    /// UI text.
    pub fn message(&self) -> String {
        match self {
            ChapterOutcome::Created => "Chapter marker added in OBS too.".into(),
            ChapterOutcome::NotConnected => {
                "OBS is not connected, so the marker is in the session log only.".into()
            }
            ChapterOutcome::NotInThisObs => "This OBS version cannot add chapter markers (OBS 30.2 or later can). The marker is in the session log.".into(),
            ChapterOutcome::NotRecording => {
                "OBS is not recording, so the marker is in the session log only.".into()
            }
            ChapterOutcome::Paused => "OBS cannot add a chapter marker while recording is paused. The marker is in the session log.".into(),
            ChapterOutcome::FormatUnsupported => "This recording format does not support chapters (use Hybrid MP4). The marker is in the session log.".into(),
            ChapterOutcome::Failed { code } => format!(
                "OBS could not add the chapter marker (code {code}). The marker is in the session log."
            ),
            ChapterOutcome::Timeout => {
                "OBS did not answer in time. The marker is in the session log.".into()
            }
        }
    }
}

/// Things the app reacts to, in order, on a broadcast channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObsEvent {
    Connected {
        obs_version: String,
        ws_version: String,
    },
    Disconnected,
    /// A record state change, stamped when it arrived.
    Record {
        state: RecordState,
        at: Instant,
    },
    /// A duration sample while recording or paused.
    Sample(DurationSample),
    /// The outcome of [`crate::ObsClient::add_chapter`].
    Chapter(ChapterOutcome),
    /// OBS is closing (`ExitStarted`).
    Exiting,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn output_states() {
        use RecordState::*;
        let cases = [
            ("OBS_WEBSOCKET_OUTPUT_STARTING", Some(Starting)),
            ("OBS_WEBSOCKET_OUTPUT_STARTED", Some(Recording)),
            ("OBS_WEBSOCKET_OUTPUT_PAUSED", Some(Paused)),
            ("OBS_WEBSOCKET_OUTPUT_RESUMED", Some(Recording)),
            ("OBS_WEBSOCKET_OUTPUT_STOPPING", Some(Stopping)),
            ("OBS_WEBSOCKET_OUTPUT_STOPPED", Some(Stopped)),
            ("OBS_WEBSOCKET_OUTPUT_UNKNOWN", None),
            ("OBS_WEBSOCKET_OUTPUT_RECONNECTING", None),
            ("", None),
        ];
        for (s, want) in cases {
            assert_eq!(RecordState::from_output_state(s), want, "{s}");
        }
        assert!(Paused.active() && Recording.active() && Stopping.active());
        assert!(!Stopped.active() && !Starting.active());
    }

    #[test]
    fn recording_wire_and_extrapolation() {
        let t0 = Instant::now();
        let r = Recording {
            state: RecordState::Recording,
            timecode: "00:01:23.450".into(),
            duration_ms: 83_450,
            sample: Some(DurationSample {
                at: t0,
                duration_ms: 83_450,
                paused: false,
            }),
        };
        assert_eq!(r.duration_at(t0 + Duration::from_millis(550)), 84_000);
        let w = r.to_wire();
        assert!(w.active && !w.paused);
        assert_eq!(w.timecode, "00:01:23.450");
        let p = Recording {
            state: RecordState::Paused,
            ..r.clone()
        };
        assert_eq!(p.duration_at(t0 + Duration::from_secs(9)), 83_450);
        assert!(p.to_wire().paused && p.to_wire().active);
        let empty = Recording::default();
        assert_eq!(empty.to_wire().timecode, "00:00:00.000");
        assert_eq!(timecode(3_723_004), "01:02:03.004");
    }

    #[test]
    fn ipc_mapping() {
        let mut s = ObsStatus {
            connection: Connection::Connected {
                obs_version: "32.2.2".into(),
                ws_version: "5.7.4".into(),
            },
            recording: Some(Recording::default()),
            chapters: ChapterSupport::Available,
        };
        let ipc = s.to_ipc(true);
        assert_eq!(ipc.state, ObsState::Connected);
        assert!(ipc.error.is_none());
        assert_eq!(ipc.recording.unwrap().timecode, "00:00:00.000");
        assert_eq!(s.to_ipc(false).state, ObsState::Off);
        assert!(s.to_ipc(false).recording.is_none());
        s.connection = Connection::AuthFailed {
            missing_password: false,
        };
        let ipc = s.to_ipc(true);
        assert_eq!(ipc.state, ObsState::AuthFailed);
        assert!(ipc.error.unwrap().contains("did not accept the password"));
        assert!(
            ipc.recording.is_none(),
            "no recording state while disconnected"
        );
        for c in [
            Connection::Kicked,
            Connection::Unsupported { reason: "x".into() },
            Connection::Error("y".into()),
        ] {
            assert_eq!(c.ipc_state(), ObsState::Error);
            assert!(c.message().is_some());
        }
        assert_eq!(Connection::Connecting.ipc_state(), ObsState::Connecting);
        assert_eq!(Connection::Off.ipc_state(), ObsState::Off);
    }

    #[test]
    fn messages_are_plain() {
        let all = [
            ChapterOutcome::Created,
            ChapterOutcome::NotConnected,
            ChapterOutcome::NotInThisObs,
            ChapterOutcome::NotRecording,
            ChapterOutcome::Paused,
            ChapterOutcome::FormatUnsupported,
            ChapterOutcome::Failed { code: 205 },
            ChapterOutcome::Timeout,
        ];
        for o in &all {
            let m = o.message();
            assert!(m.ends_with('.'));
            assert!(!m.contains('\u{2014}') && !m.contains('\u{2013}'));
        }
        assert!(ChapterOutcome::FormatUnsupported
            .message()
            .contains("does not support chapters (use Hybrid MP4)"));
        for c in [
            Connection::AuthFailed {
                missing_password: true,
            },
            Connection::Kicked,
        ] {
            let m = c.message().unwrap();
            assert!(!m.contains('\u{2014}'));
        }
    }
}
