//! The OBS bridge: a keystage-obs client while `settings.obs.enabled`, with
//! the password decrypted from `settings.obs.password_dpapi` (DPAPI, this
//! Windows account only). Recording state flows into `AppState.recording`
//! and `Status.obs`; record state changes and duration samples go into the
//! running Session Log so its times map to video time; with
//! `obs.auto_session` a recording start starts a Session Log and its stop
//! ends the one it started. Chapter markers are mirrored into OBS with
//! `CreateRecordChapter` when OBS supports it (best effort).
//!
//! Privacy: the password is never logged or kept beyond the client config;
//! chapter names and chapter outcome messages are never logged.

use std::time::{Duration, Instant};

use keystage_core::ipc::ObsStatus as IpcObsStatus;
use keystage_core::protocol::RecordingState;
use keystage_core::settings::ObsSettings;
use keystage_obs::{ChapterOutcome, ObsClient, ObsConfig, ObsEvent, ObsStatus, RecordState};
use keystage_session::RecordEvent;
use parking_lot::Mutex;

use crate::platform;

/// What a recording state change means for the Session Log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStep {
    /// Start a Session Log (auto sessions) before recording `Started`.
    StartSession,
    /// Record this OBS event in the running Session Log.
    Record(RecordEvent),
    /// Stop the Session Log OBS started (auto sessions), after `Stopped`.
    StopSession,
}

/// Turns `RecordState` changes into Session Log steps.
#[derive(Clone, Copy, Debug, Default)]
pub struct RecordTracker {
    last: RecordState,
}

impl RecordTracker {
    pub fn last(&self) -> RecordState {
        self.last
    }

    /// The steps for a new state. `auto_session` is `settings.obs.auto_session`.
    pub fn on_state(&mut self, state: RecordState, auto_session: bool) -> Vec<SessionStep> {
        let prev = self.last;
        self.last = state;
        let mut out = Vec::new();
        match state {
            RecordState::Recording => match prev {
                RecordState::Paused => out.push(SessionStep::Record(RecordEvent::Resumed)),
                RecordState::Recording => {}
                RecordState::Stopped | RecordState::Starting | RecordState::Stopping => {
                    if auto_session {
                        out.push(SessionStep::StartSession);
                    }
                    out.push(SessionStep::Record(RecordEvent::Started));
                }
            },
            RecordState::Paused => {
                if prev != RecordState::Paused {
                    out.push(SessionStep::Record(RecordEvent::Paused));
                }
            }
            RecordState::Stopped => {
                if matches!(
                    prev,
                    RecordState::Recording | RecordState::Paused | RecordState::Stopping
                ) {
                    out.push(SessionStep::Record(RecordEvent::Stopped));
                    if auto_session {
                        out.push(SessionStep::StopSession);
                    }
                }
            }
            RecordState::Starting | RecordState::Stopping => {}
        }
        out
    }

    /// OBS went away: the recording state is unknown from here on.
    pub fn reset(&mut self) {
        self.last = RecordState::Stopped;
    }
}

/// An `Instant` from the OBS client on the app's monotonic millisecond clock.
pub fn instant_to_ms(at: Instant, now: Instant, now_ms: u64) -> u64 {
    let ago = now.saturating_duration_since(at).as_millis();
    now_ms.saturating_sub(u64::try_from(ago).unwrap_or(u64::MAX))
}

/// Chapter outcomes worth telling the user once (the marker is always in
/// the Session Log).
pub fn chapter_notice(o: &ChapterOutcome) -> Option<String> {
    match o {
        ChapterOutcome::FormatUnsupported | ChapterOutcome::NotInThisObs => Some(o.message()),
        _ => None,
    }
}

/// What the client is configured with (no password: only whether one is set).
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClientKey {
    host: String,
    port: u16,
    blob: Option<String>,
}

/// The client config and a notice when the stored password could not be read.
pub fn client_config(obs: &ObsSettings) -> (ObsConfig, Option<String>) {
    let (password, notice) = match obs.password_dpapi.as_deref() {
        None => (None, None),
        Some(blob) => match platform::dpapi_unprotect(blob) {
            Ok(p) => (Some(p), None),
            Err(_) => (
                None,
                Some(
                    "The saved OBS password could not be read on this Windows account. Enter it again in settings."
                        .to_string(),
                ),
            ),
        },
    };
    (ObsConfig::new(obs.host.clone(), obs.port, password), notice)
}

/// Owns the OBS client.
#[derive(Default)]
pub struct ObsBridge {
    client: Mutex<Option<(ObsClient, ClientKey, u64)>>,
    generation: std::sync::atomic::AtomicU64,
    notice: Mutex<Option<String>>,
    chapter_notice: Mutex<Option<String>>,
}

/// What `apply` did, so the shell can attach event listeners to a new client.
pub enum BridgeChange {
    Unchanged,
    Stopped,
    Started {
        generation: u64,
        events: tokio::sync::broadcast::Receiver<ObsEvent>,
        status: tokio::sync::watch::Receiver<ObsStatus>,
    },
}

impl ObsBridge {
    pub fn new() -> ObsBridge {
        ObsBridge::default()
    }

    /// Starts, reconfigures or stops the client for these settings.
    pub fn apply(&self, obs: &ObsSettings) -> BridgeChange {
        let mut slot = self.client.lock();
        if !obs.enabled {
            *self.notice.lock() = None;
            if let Some((c, _, _)) = slot.take() {
                drop(slot);
                c.shutdown();
                tracing::info!("obs client stopped");
                return BridgeChange::Stopped;
            }
            return BridgeChange::Unchanged;
        }
        let key = ClientKey {
            host: obs.host.clone(),
            port: obs.port,
            blob: obs.password_dpapi.clone(),
        };
        if let Some((c, current, _)) = slot.as_mut() {
            if *current != key {
                let (cfg, notice) = client_config(obs);
                *self.notice.lock() = notice;
                c.set_config(cfg);
                *current = key;
            }
            return BridgeChange::Unchanged;
        }
        let (cfg, notice) = client_config(obs);
        *self.notice.lock() = notice;
        match ObsClient::spawn(cfg) {
            Ok(c) => {
                let generation = self
                    .generation
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                    + 1;
                let events = c.events();
                let status = c.watch();
                *slot = Some((c, key, generation));
                tracing::info!("obs client started");
                BridgeChange::Started {
                    generation,
                    events,
                    status,
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "obs client did not start");
                *self.notice.lock() =
                    Some("The OBS connection could not be started. Try again.".into());
                BridgeChange::Unchanged
            }
        }
    }

    /// True while `generation` is the running client.
    pub fn is_current(&self, generation: u64) -> bool {
        self.client
            .lock()
            .as_ref()
            .is_some_and(|(_, _, g)| *g == generation)
    }

    /// `Status.obs`.
    pub fn status(&self, enabled: bool) -> IpcObsStatus {
        match self.client.lock().as_ref() {
            Some((c, _, _)) => c.status().to_ipc(enabled),
            None => ObsStatus::default().to_ipc(enabled),
        }
    }

    /// `AppState.recording`: `None` unless connected.
    pub fn recording(&self) -> Option<RecordingState> {
        let slot = self.client.lock();
        let (c, _, _) = slot.as_ref()?;
        let st = c.status();
        if !st.connection.is_connected() {
            return None;
        }
        st.recording.as_ref().map(|r| r.to_wire())
    }

    /// Mirrors a chapter marker into OBS (fire and forget).
    pub fn add_chapter(&self, name: Option<&str>) {
        if let Some((c, _, _)) = self.client.lock().as_ref() {
            c.add_chapter(name);
        }
    }

    /// Reconnects now (after OBS kicked Keystage, or to skip a wait).
    pub fn reconnect(&self) {
        if let Some((c, _, _)) = self.client.lock().as_ref() {
            c.reconnect();
        }
    }

    pub fn set_chapter_notice(&self, n: Option<String>) {
        *self.chapter_notice.lock() = n;
    }

    /// Notices for the status.
    pub fn notices(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(n) = self.notice.lock().clone() {
            v.push(n);
        }
        if let Some(n) = self.chapter_notice.lock().clone() {
            v.push(n);
        }
        v
    }

    /// Disconnects.
    pub fn shutdown(&self) {
        let c = self.client.lock().take();
        if let Some((c, _, _)) = c {
            c.shutdown();
        }
    }
}

/// How long `obs_test` waits.
pub const TEST_LIMIT: Duration = Duration::from_secs(5);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_maps_to_session_steps() {
        let mut t = RecordTracker::default();
        assert!(t.on_state(RecordState::Starting, true).is_empty());
        assert_eq!(
            t.on_state(RecordState::Recording, true),
            vec![
                SessionStep::StartSession,
                SessionStep::Record(RecordEvent::Started)
            ]
        );
        assert_eq!(
            t.on_state(RecordState::Paused, true),
            vec![SessionStep::Record(RecordEvent::Paused)]
        );
        assert_eq!(
            t.on_state(RecordState::Recording, true),
            vec![SessionStep::Record(RecordEvent::Resumed)]
        );
        assert!(t.on_state(RecordState::Stopping, true).is_empty());
        assert_eq!(
            t.on_state(RecordState::Stopped, true),
            vec![
                SessionStep::Record(RecordEvent::Stopped),
                SessionStep::StopSession
            ]
        );
        assert!(t.on_state(RecordState::Stopped, true).is_empty());
    }

    #[test]
    fn without_auto_sessions_only_events_are_recorded() {
        let mut t = RecordTracker::default();
        assert_eq!(
            t.on_state(RecordState::Recording, false),
            vec![SessionStep::Record(RecordEvent::Started)]
        );
        assert_eq!(
            t.on_state(RecordState::Stopped, false),
            vec![SessionStep::Record(RecordEvent::Stopped)]
        );
        // A seed that finds OBS already paused records the pause only.
        let mut u = RecordTracker::default();
        assert_eq!(
            u.on_state(RecordState::Paused, false),
            vec![SessionStep::Record(RecordEvent::Paused)]
        );
        u.reset();
        assert_eq!(u.last(), RecordState::Stopped);
    }

    #[test]
    fn instants_map_onto_the_monotonic_clock() {
        let now = Instant::now();
        let at = now - Duration::from_millis(250);
        assert_eq!(instant_to_ms(at, now, 10_000), 9_750);
        assert_eq!(
            instant_to_ms(now + Duration::from_secs(1), now, 10_000),
            10_000
        );
        assert_eq!(instant_to_ms(at, now, 100), 0);
    }

    #[test]
    fn only_lasting_chapter_problems_become_notices() {
        assert!(chapter_notice(&ChapterOutcome::FormatUnsupported)
            .expect("n")
            .contains("Hybrid MP4"));
        assert!(chapter_notice(&ChapterOutcome::NotInThisObs).is_some());
        assert!(chapter_notice(&ChapterOutcome::Created).is_none());
        assert!(chapter_notice(&ChapterOutcome::NotRecording).is_none());
    }

    #[test]
    fn a_disabled_bridge_reports_off() {
        let b = ObsBridge::new();
        assert!(matches!(
            b.apply(&ObsSettings::default()),
            BridgeChange::Unchanged
        ));
        let st = b.status(false);
        assert_eq!(st.state, keystage_core::ipc::ObsState::Off);
        assert!(b.recording().is_none());
        b.add_chapter(Some("x"));
        b.shutdown();
    }

    #[test]
    fn an_unreadable_password_is_a_notice_not_plain_text() {
        let obs = ObsSettings {
            enabled: true,
            password_dpapi: Some("QUJD".into()),
            ..ObsSettings::default()
        };
        let (cfg, notice) = client_config(&obs);
        // Off Windows there is no DPAPI; on Windows this blob is not ours.
        assert!(cfg.password.is_none());
        assert!(notice.expect("notice").contains("could not be read"));
        let (cfg, notice) = client_config(&ObsSettings::default());
        assert!(cfg.password.is_none() && notice.is_none());
        assert_eq!(cfg.port, 4455);
    }

    #[test]
    fn an_enabled_bridge_starts_and_stops_a_client() {
        // Nothing listens on this port: the client keeps retrying quietly.
        let obs = ObsSettings {
            enabled: true,
            host: "127.0.0.1".into(),
            port: 9,
            ..ObsSettings::default()
        };
        let b = ObsBridge::new();
        let g = match b.apply(&obs) {
            BridgeChange::Started { generation, .. } => generation,
            _ => panic!("expected a new client"),
        };
        assert!(b.is_current(g));
        assert!(matches!(b.apply(&obs), BridgeChange::Unchanged));
        assert!(b.recording().is_none());
        let off = ObsSettings {
            enabled: false,
            ..obs
        };
        assert!(matches!(b.apply(&off), BridgeChange::Stopped));
        assert!(!b.is_current(g));
    }
}
