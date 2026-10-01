//! The Session Log (ARCHITECTURE.md section 10): the callouts that were
//! displayed (final state of each), chapter markers, and what OBS reported
//! about the recording. [`SessionLog`] records a live session;
//! [`Session`] is the stored form (`keystage.session/1`), which the app
//! writes only through [`Session::save`] when it decides to.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use keystage_core::ipc::{SessionStatus, SessionSummary};
use keystage_core::protocol::{SessionState, APP_NAME, APP_VERSION};
use keystage_core::Callout;
use serde::{Deserialize, Serialize};

use crate::clock::{ClockEvent, ClockSample, RecordEvent, RecordingLog};
use crate::time;

/// The schema id written into every session file.
pub const SESSION_SCHEMA: &str = "keystage.session/1";
/// The schema family this version reads (`keystage.session/<major>`).
const SCHEMA_PREFIX: &str = "keystage.session/";
const SCHEMA_MAJOR: &str = "1";

/// Most callouts kept in one session; later new callouts are dropped and
/// the session is marked `truncated`. About ten hours of steady shortcut use.
pub const MAX_CALLOUTS: usize = 100_000;
/// Most chapter markers in one session.
pub const MAX_MARKERS: usize = 1_000;
/// Longest marker name, in characters.
pub const MAX_MARKER_NAME: usize = 100;
/// Most stored OBS events and duration samples.
pub const MAX_CLOCK_EVENTS: usize = 10_000;
pub const MAX_CLOCK_SAMPLES: usize = 20_000;
/// Largest session file [`Session::load`] reads.
pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;

/// A duration sample is stored only when the mapping built from earlier
/// samples is off by more than this ...
const SAMPLE_TOLERANCE_MS: u64 = 20;
/// ... or when this long has passed since the last stored sample.
const SAMPLE_REFRESH_MS: u64 = 15_000;

/// Why a session operation failed.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("there is no chapter marker number {index}")]
    NoSuchMarker { index: usize },
    #[error("a chapter name cannot be empty")]
    EmptyName,
    #[error("the session log is stopped")]
    Stopped,
    #[error("too many chapter markers (the limit is {MAX_MARKERS})")]
    TooManyMarkers,
    #[error("could not read or write the session file: {0}")]
    Io(#[from] io::Error),
    #[error("the session file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the session file uses format \"{found}\", which this version cannot read")]
    UnsupportedSchema { found: String },
    #[error("the session file is larger than {MAX_FILE_BYTES} bytes")]
    TooLarge,
}

/// The app that wrote a session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

impl Default for AppInfo {
    fn default() -> Self {
        AppInfo {
            name: APP_NAME.to_string(),
            version: APP_VERSION.to_string(),
        }
    }
}

/// A chapter marker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// Monotonic time the marker was added (same clock as callouts).
    pub t_ms: u64,
    pub name: String,
}

/// A stored session, `keystage.session/1`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub schema: String,
    pub id: String,
    #[serde(default)]
    pub app: AppInfo,
    /// Start, ISO 8601 UTC.
    pub started: String,
    pub started_unix_ms: u64,
    /// Stop, ISO 8601 UTC; `None` while the session runs.
    #[serde(default)]
    pub ended: Option<String>,
    #[serde(default)]
    pub ended_unix_ms: Option<u64>,
    /// Monotonic start time (the clock of every `t_ms` in the file).
    pub start_ms: u64,
    #[serde(default)]
    pub end_ms: Option<u64>,
    /// What OBS reported about the recording; empty without OBS.
    #[serde(default)]
    pub recording: RecordingLog,
    /// In time order.
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// Final state of each callout that was displayed, in the order they
    /// first appeared. Exactly the wire `callout` objects.
    #[serde(default)]
    pub callouts: Vec<Callout>,
    /// True when [`MAX_CALLOUTS`] was reached and later callouts were dropped.
    #[serde(default)]
    pub truncated: bool,
    /// Fields written by a newer 1.x version, kept so a rename round trip
    /// does not lose them.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Collapses whitespace (newlines included) and trims a user-supplied name.
pub(crate) fn clean_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for word in raw.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().filter(|c| !c.is_control()));
    }
    if out.chars().count() > MAX_MARKER_NAME {
        out = out.chars().take(MAX_MARKER_NAME).collect::<String>();
        out.truncate(out.trim_end().len());
    }
    out
}

impl Session {
    /// A new, empty, running session.
    pub fn new(id: impl Into<String>, started_unix_ms: u64, start_ms: u64) -> Session {
        Session {
            schema: SESSION_SCHEMA.to_string(),
            id: id.into(),
            app: AppInfo::default(),
            started: time::iso_utc(started_unix_ms),
            started_unix_ms,
            ended: None,
            ended_unix_ms: None,
            start_ms,
            end_ms: None,
            recording: RecordingLog::default(),
            markers: Vec::new(),
            callouts: Vec::new(),
            truncated: false,
            extra: BTreeMap::new(),
        }
    }

    /// A file-name-safe id from the start time (UTC): `2026-10-01T14-03-22Z`.
    pub fn default_id(started_unix_ms: u64) -> String {
        time::session_id(started_unix_ms)
    }

    /// True while the session has not been stopped.
    pub fn is_running(&self) -> bool {
        self.end_ms.is_none()
    }

    /// Session length; for a running session, up to `now_ms` (monotonic).
    pub fn duration_ms(&self, now_ms: u64) -> u64 {
        self.end_ms.unwrap_or(now_ms).saturating_sub(self.start_ms)
    }

    /// Adds a marker at `t_ms`, kept in time order. An empty or missing name
    /// becomes `Chapter <n>`. Returns the marker's index.
    pub fn add_marker(&mut self, t_ms: u64, name: Option<&str>) -> Result<usize, SessionError> {
        if self.markers.len() >= MAX_MARKERS {
            return Err(SessionError::TooManyMarkers);
        }
        let mut clean = name.map(clean_name).unwrap_or_default();
        if clean.is_empty() {
            clean = format!("Chapter {}", self.markers.len() + 1);
        }
        let index = self.markers.partition_point(|m| m.t_ms <= t_ms);
        self.markers.insert(index, Marker { t_ms, name: clean });
        Ok(index)
    }

    /// Renames marker `index` (0-based, time order).
    pub fn rename_marker(&mut self, index: usize, name: &str) -> Result<(), SessionError> {
        let clean = clean_name(name);
        if clean.is_empty() {
            return Err(SessionError::EmptyName);
        }
        let marker = self
            .markers
            .get_mut(index)
            .ok_or(SessionError::NoSuchMarker { index })?;
        marker.name = clean;
        Ok(())
    }

    /// Removes marker `index` and returns it.
    pub fn remove_marker(&mut self, index: usize) -> Result<Marker, SessionError> {
        if index >= self.markers.len() {
            return Err(SessionError::NoSuchMarker { index });
        }
        Ok(self.markers.remove(index))
    }

    /// The `sessions_list` entry. `now_ms` is used only for a running session.
    pub fn summary(&self, now_ms: u64) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            started: self.started.clone(),
            duration_ms: self.duration_ms(now_ms),
            callouts: count_u32(self.callouts.len()),
            markers: count_u32(self.markers.len()),
        }
    }

    /// Pretty JSON with a trailing newline (the file and the JSON export).
    pub fn to_json_pretty(&self) -> String {
        let mut s = serde_json::to_string_pretty(self)
            .unwrap_or_else(|_| String::from("{\"schema\":\"keystage.session/1\"}"));
        s.push('\n');
        s
    }

    /// Reads a session from JSON. The schema must be `keystage.session/1`
    /// (any `keystage.session/1.x` is accepted; unknown fields are kept).
    pub fn from_json(s: &str) -> Result<Session, SessionError> {
        let s = s.strip_prefix('\u{feff}').unwrap_or(s);
        // Check the schema first so a newer major version gets a clear error.
        let probe: serde_json::Value = serde_json::from_str(s)?;
        let found = probe
            .get("schema")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = found.strip_prefix(SCHEMA_PREFIX).unwrap_or("");
        let major = version.split('.').next().unwrap_or("");
        if major != SCHEMA_MAJOR {
            return Err(SessionError::UnsupportedSchema { found });
        }
        let mut session: Session = serde_json::from_value(probe)?;
        session.markers.sort_by_key(|m| m.t_ms);
        Ok(session)
    }

    /// Writes the session to `path` as pretty JSON: to a temporary file in
    /// the same folder first, then renamed over the target, so a crash never
    /// leaves half a file. Creates the folder if needed.
    pub fn save(&self, path: &Path) -> Result<(), SessionError> {
        let dir = match path.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        fs::create_dir_all(&dir)?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "session.json".to_string());
        let tmp = dir.join(format!(".{file_name}.tmp"));
        let json = self.to_json_pretty();
        let result = (|| -> io::Result<()> {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result.map_err(SessionError::from)
    }

    /// Reads a session file written by [`Session::save`].
    pub fn load(path: &Path) -> Result<Session, SessionError> {
        let meta = fs::metadata(path)?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(SessionError::TooLarge);
        }
        let bytes = fs::read(path)?;
        let text = String::from_utf8(bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.utf8_error()))?;
        Session::from_json(&text)
    }
}

fn count_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A live Session Log. The app feeds it exactly the `callout` messages the
/// renderer was sent, plus markers and OBS record state; nothing else.
#[derive(Clone, Debug)]
pub struct SessionLog {
    session: Session,
    /// `(callout id, callout t_ms)` to its index in `session.callouts`.
    /// Keyed by the creation time too, so a processor restart that reuses
    /// ids never overwrites an earlier callout.
    index: HashMap<(u64, u64), usize>,
    last_sample: Option<ClockSample>,
}

impl SessionLog {
    /// Starts a session. `started_unix_ms` is wall time (for the file),
    /// `t_ms` the monotonic time callouts use.
    pub fn start(id: impl Into<String>, started_unix_ms: u64, t_ms: u64) -> SessionLog {
        SessionLog {
            session: Session::new(id, started_unix_ms, t_ms),
            index: HashMap::new(),
            last_sample: None,
        }
    }

    /// Continues a stored session that was never stopped (for example after
    /// a crash): it is reopened as is. A stopped session stays stopped.
    pub fn resume(session: Session) -> SessionLog {
        let index = session
            .callouts
            .iter()
            .enumerate()
            .map(|(i, c)| ((c.id, c.t_ms), i))
            .collect();
        let last_sample = session.recording.samples.last().copied();
        SessionLog {
            session,
            index,
            last_sample,
        }
    }

    pub fn is_active(&self) -> bool {
        self.session.is_running()
    }

    pub fn id(&self) -> &str {
        &self.session.id
    }

    /// Records a displayed callout, replacing an earlier state of the same
    /// callout. Returns false when the log is stopped or full.
    pub fn add_callout(&mut self, callout: &Callout) -> bool {
        if !self.is_active() {
            return false;
        }
        let key = (callout.id, callout.t_ms);
        if let Some(&i) = self.index.get(&key) {
            // Keep the latest state; an older update arriving late is ignored.
            if callout.updated_ms >= self.session.callouts[i].updated_ms {
                self.session.callouts[i] = callout.clone();
            }
            return true;
        }
        if self.session.callouts.len() >= MAX_CALLOUTS {
            self.session.truncated = true;
            return false;
        }
        self.index.insert(key, self.session.callouts.len());
        self.session.callouts.push(callout.clone());
        true
    }

    /// Adds a chapter marker at `t_ms`. Returns its index.
    pub fn add_marker(&mut self, t_ms: u64, name: Option<&str>) -> Result<usize, SessionError> {
        if !self.is_active() {
            return Err(SessionError::Stopped);
        }
        self.session.add_marker(t_ms, name)
    }

    /// Renames a marker (allowed after the session stopped, too).
    pub fn rename_marker(&mut self, index: usize, name: &str) -> Result<(), SessionError> {
        self.session.rename_marker(index, name)
    }

    /// Records an OBS record state change at monotonic time `t_ms`.
    pub fn record_event(&mut self, t_ms: u64, event: RecordEvent) {
        if !self.is_active() || self.session.recording.events.len() >= MAX_CLOCK_EVENTS {
            return;
        }
        self.session
            .recording
            .events
            .push(ClockEvent { t_ms, event });
    }

    /// Offers an OBS duration sample (`outputDuration` at monotonic `t_ms`).
    /// Only samples that change the mapping are stored: the first, one whose
    /// paused flag differs, one the previous sample does not predict within
    /// 20 ms, and one every 15 s. A duration of 0 (OBS reports 0 while the
    /// output is not active) is never stored. Returns true when it was stored.
    pub fn clock_sample(&mut self, t_ms: u64, duration_ms: u64, paused: bool) -> bool {
        if !self.is_active()
            || duration_ms == 0
            || self.session.recording.samples.len() >= MAX_CLOCK_SAMPLES
        {
            return false;
        }
        let sample = ClockSample {
            t_ms,
            duration_ms,
            paused,
        };
        let store = match self.last_sample {
            None => true,
            Some(last) if t_ms < last.t_ms => false,
            Some(last) => {
                let predicted = if last.paused {
                    last.duration_ms
                } else {
                    last.duration_ms + (t_ms - last.t_ms)
                };
                last.paused != paused
                    || predicted.abs_diff(duration_ms) > SAMPLE_TOLERANCE_MS
                    || t_ms - last.t_ms >= SAMPLE_REFRESH_MS
            }
        };
        if store {
            self.session.recording.samples.push(sample);
            self.last_sample = Some(sample);
        }
        store
    }

    /// Stops the session. Later callouts and markers are ignored.
    pub fn stop(&mut self, t_ms: u64, ended_unix_ms: u64) {
        if !self.is_active() {
            return;
        }
        self.session.end_ms = Some(t_ms.max(self.session.start_ms));
        self.session.ended_unix_ms = Some(ended_unix_ms);
        self.session.ended = Some(time::iso_utc(ended_unix_ms));
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn into_session(self) -> Session {
        self.session
    }

    /// `AppState.session` for the overlay.
    pub fn state(&self) -> SessionState {
        SessionState {
            active: self.is_active(),
            started_ms: self.session.start_ms,
            markers: count_u32(self.session.markers.len()),
        }
    }

    /// `Status.session` for the settings window.
    pub fn status(&self) -> SessionStatus {
        SessionStatus {
            id: self.session.id.clone(),
            active: self.is_active(),
            started: self.session.started.clone(),
            callouts: count_u32(self.session.callouts.len()),
            markers: count_u32(self.session.markers.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::{CalloutKind, Chip, ChipKind};

    pub(crate) fn callout(id: u64, t_ms: u64, updated_ms: u64, count: u32) -> Callout {
        Callout {
            id,
            kind: CalloutKind::Combo,
            steps: vec![vec![
                Chip::new("Ctrl", "Ctrl", ChipKind::Modifier),
                Chip::new("K", "K", ChipKind::Key),
            ]],
            text: None,
            count,
            caption: Some("Fixture caption".into()),
            context: None,
            profile: Some("fixture".into()),
            t_ms,
            updated_ms,
        }
    }

    #[test]
    fn upsert_keeps_final_state() {
        let mut log = SessionLog::start("s", 1_790_812_800_000, 1_000);
        assert!(log.add_callout(&callout(1, 1_100, 1_100, 1)));
        assert!(log.add_callout(&callout(1, 1_100, 1_400, 2)));
        assert!(log.add_callout(&callout(1, 1_100, 1_900, 4)));
        // A late, older update does not win.
        assert!(log.add_callout(&callout(1, 1_100, 1_200, 2)));
        assert!(log.add_callout(&callout(2, 2_000, 2_000, 1)));
        let s = log.session();
        assert_eq!(s.callouts.len(), 2);
        assert_eq!(s.callouts[0].count, 4);
        assert_eq!(s.callouts[0].updated_ms, 1_900);
        assert_eq!(s.callouts[1].id, 2);
    }

    #[test]
    fn reused_ids_do_not_overwrite() {
        let mut log = SessionLog::start("s", 0, 0);
        log.add_callout(&callout(1, 100, 100, 1));
        // The processor was rebuilt and counts ids from 1 again.
        log.add_callout(&callout(1, 5_000, 5_000, 1));
        assert_eq!(log.session().callouts.len(), 2);
    }

    #[test]
    fn stop_freezes_the_log() {
        let mut log = SessionLog::start("s", 1_790_812_800_000, 1_000);
        assert!(log.is_active());
        log.add_callout(&callout(1, 1_100, 1_100, 1));
        log.stop(61_000, 1_790_812_860_000);
        assert!(!log.is_active());
        assert!(!log.add_callout(&callout(2, 62_000, 62_000, 1)));
        assert!(matches!(
            log.add_marker(62_000, Some("Late")),
            Err(SessionError::Stopped)
        ));
        log.record_event(62_000, RecordEvent::Started);
        assert!(!log.clock_sample(62_000, 10, false));
        // Stopping twice keeps the first stop.
        log.stop(99_000, 1_790_812_900_000);
        let s = log.session();
        assert_eq!(s.end_ms, Some(61_000));
        assert_eq!(s.ended.as_deref(), Some("2026-10-01T00:01:00.000Z"));
        assert_eq!(s.duration_ms(0), 60_000);
        assert!(s.recording.is_empty());
        assert_eq!(s.callouts.len(), 1);
    }

    #[test]
    fn markers_default_names_order_and_rename() {
        let mut log = SessionLog::start("s", 0, 0);
        assert_eq!(log.add_marker(5_000, None).unwrap(), 0);
        assert_eq!(
            log.add_marker(9_000, Some("  Sketch\nthe   base "))
                .unwrap(),
            1
        );
        // Out of order insert lands in time order.
        assert_eq!(log.add_marker(7_000, Some("")).unwrap(), 1);
        let names: Vec<&str> = log
            .session()
            .markers
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["Chapter 1", "Chapter 3", "Sketch the base"]);
        log.rename_marker(1, "Extrude").unwrap();
        assert_eq!(log.session().markers[1].name, "Extrude");
        assert!(matches!(
            log.rename_marker(1, " \t "),
            Err(SessionError::EmptyName)
        ));
        assert!(matches!(
            log.rename_marker(9, "x"),
            Err(SessionError::NoSuchMarker { index: 9 })
        ));
        let mut s = log.into_session();
        let removed = s.remove_marker(0).unwrap();
        assert_eq!(removed.name, "Chapter 1");
        assert_eq!(s.markers.len(), 2);
        assert!(s.remove_marker(5).is_err());
    }

    #[test]
    fn long_names_are_cut_at_a_character_boundary() {
        let long = "é".repeat(150);
        let clean = clean_name(&long);
        assert_eq!(clean.chars().count(), MAX_MARKER_NAME);
        let spaced = format!("{} tail", "a".repeat(99));
        assert_eq!(clean_name(&spaced), "a".repeat(99));
        assert_eq!(clean_name("a\u{7}b"), "ab");
    }

    #[test]
    fn marker_limit() {
        let mut s = Session::new("s", 0, 0);
        for i in 0..MAX_MARKERS {
            s.add_marker(i as u64, None).unwrap();
        }
        assert!(matches!(
            s.add_marker(0, None),
            Err(SessionError::TooManyMarkers)
        ));
    }

    #[test]
    fn callout_limit_marks_truncated() {
        let mut log = SessionLog::start("s", 0, 0);
        for i in 0..MAX_CALLOUTS as u64 {
            assert!(log.add_callout(&callout(i, i, i, 1)));
        }
        assert!(!log.add_callout(&callout(u64::MAX, 1, 1, 1)));
        // Updates to known callouts still apply.
        assert!(log.add_callout(&callout(0, 0, 10, 3)));
        assert!(log.session().truncated);
        assert_eq!(log.session().callouts[0].count, 3);
    }

    #[test]
    fn sample_thinning() {
        let mut log = SessionLog::start("s", 0, 0);
        assert!(!log.clock_sample(500, 0, false), "output not active");
        assert!(log.clock_sample(1_000, 900, false), "first");
        assert!(
            !log.clock_sample(2_000, 1_905, false),
            "predicted within 20 ms"
        );
        assert!(!log.clock_sample(3_000, 2_880, false));
        assert!(log.clock_sample(4_000, 3_950, false), "off by 50 ms");
        assert!(!log.clock_sample(3_500, 3_400, false), "out of order");
        assert!(log.clock_sample(5_000, 4_950, true), "paused flag changed");
        assert!(
            !log.clock_sample(6_000, 4_950, true),
            "still paused, same time"
        );
        assert!(log.clock_sample(7_000, 4_960, false), "resumed");
        assert!(!log.clock_sample(8_000, 5_960, false));
        assert!(
            log.clock_sample(22_000, 19_960, false),
            "refresh after 15 s"
        );
        assert_eq!(log.session().recording.samples.len(), 5);
    }

    #[test]
    fn state_status_summary() {
        let mut log = SessionLog::start("2026-10-01T00-00-00Z", 1_790_812_800_000, 500);
        log.add_callout(&callout(1, 600, 600, 1));
        log.add_marker(700, None).unwrap();
        let st = log.state();
        assert!(st.active);
        assert_eq!(st.started_ms, 500);
        assert_eq!(st.markers, 1);
        let status = log.status();
        assert_eq!(status.started, "2026-10-01T00:00:00.000Z");
        assert_eq!(status.callouts, 1);
        let sum = log.session().summary(10_500);
        assert_eq!(sum.duration_ms, 10_000);
        assert_eq!(sum.id, "2026-10-01T00-00-00Z");
        assert_eq!(
            Session::default_id(1_790_812_800_000),
            "2026-10-01T00-00-00Z"
        );
    }

    #[test]
    fn json_round_trip_and_schema_checks() {
        let mut log = SessionLog::start("s", 1_790_812_800_000, 0);
        log.add_callout(&callout(1, 10, 20, 2));
        log.add_marker(15, Some("One")).unwrap();
        log.record_event(5, RecordEvent::Started);
        log.clock_sample(6, 1, false);
        log.stop(100, 1_790_812_800_100);
        let s = log.into_session();
        let json = s.to_json_pretty();
        assert!(json.ends_with("}\n"));
        let back = Session::from_json(&json).unwrap();
        assert_eq!(back, s);
        // A BOM is tolerated.
        assert_eq!(Session::from_json(&format!("\u{feff}{json}")).unwrap(), s);

        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v["schema"] = "keystage.session/2".into();
        let err = Session::from_json(&v.to_string()).unwrap_err();
        assert!(
            matches!(err, SessionError::UnsupportedSchema { ref found } if found == "keystage.session/2")
        );
        assert!(err.to_string().contains("keystage.session/2"));
        v["schema"] = "keystage.session/1.3".into();
        v["future_field"] = serde_json::json!({"a": 1});
        let newer = Session::from_json(&v.to_string()).unwrap();
        assert_eq!(newer.extra["future_field"]["a"], 1);
        assert!(newer.to_json_pretty().contains("future_field"));
        v.as_object_mut().unwrap().remove("schema");
        assert!(matches!(
            Session::from_json(&v.to_string()),
            Err(SessionError::UnsupportedSchema { .. })
        ));
        assert!(matches!(
            Session::from_json("not json"),
            Err(SessionError::Json(_))
        ));
    }

    #[test]
    fn minimal_file_loads_with_defaults() {
        let s = Session::from_json(
            r#"{"schema":"keystage.session/1","id":"x","started":"2026-10-01T00:00:00.000Z","started_unix_ms":0,"start_ms":0}"#,
        )
        .unwrap();
        assert!(s.callouts.is_empty() && s.markers.is_empty() && s.recording.is_empty());
        assert!(s.is_running());
        assert_eq!(s.app.name, "IDEA Keystage");
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("2026-10-01T00-00-00Z.json");
        let mut log = SessionLog::start("2026-10-01T00-00-00Z", 1_790_812_800_000, 0);
        log.add_callout(&callout(1, 10, 20, 2));
        log.stop(100, 1_790_812_800_100);
        let s = log.into_session();
        s.save(&path).unwrap();
        // No temporary file is left behind.
        let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["2026-10-01T00-00-00Z.json"]);
        let back = Session::load(&path).unwrap();
        assert_eq!(back, s);
        // Saving again replaces the file.
        let mut renamed = back.clone();
        renamed.id = "other".into();
        renamed.save(&path).unwrap();
        assert_eq!(Session::load(&path).unwrap().id, "other");
        assert!(matches!(
            Session::load(&dir.path().join("missing.json")),
            Err(SessionError::Io(_))
        ));
        std::fs::write(dir.path().join("bad.json"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(
            Session::load(&dir.path().join("bad.json")),
            Err(SessionError::Io(_))
        ));
    }

    #[test]
    fn resume_continues_an_unstopped_session() {
        let mut log = SessionLog::start("s", 0, 0);
        log.add_callout(&callout(1, 10, 10, 1));
        log.clock_sample(20, 5, false);
        let s = log.into_session();
        let mut again = SessionLog::resume(s);
        assert!(again.is_active());
        assert!(again.add_callout(&callout(1, 10, 30, 2)));
        assert_eq!(again.session().callouts.len(), 1);
        assert_eq!(again.session().callouts[0].count, 2);
        assert!(
            !again.clock_sample(1_020, 1_005, false),
            "predicted by the stored sample"
        );
    }
}
