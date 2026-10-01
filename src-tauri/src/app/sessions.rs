//! Session Logs (ARCHITECTURE.md section 10) as the app runs them: one live
//! `SessionLog` at a time, started and stopped by hotkey, tray, settings or
//! OBS, saved as `<id>.json` in the sessions folder (Documents\IDEA
//! Keystage\Sessions unless `settings.session.folder` says otherwise) when it
//! stops and `settings.session.autosave` is on. With autosave off the stopped
//! session stays in memory for this run so it can still be exported.
//!
//! Privacy: a session holds only the callouts that were displayed and the
//! chapter markers; nothing here logs their content. Session ids that come
//! from the settings window are checked to be plain file names, so a request
//! can never reach outside the sessions folder.

use std::fs;
use std::path::{Path, PathBuf};

use keystage_core::ipc::{ExportFormat, SessionStatus, SessionSummary};
use keystage_core::protocol::SessionState;
use keystage_core::Callout;
use keystage_session::{ExportOptions, RecordEvent, Session, SessionLog};

use super::settings_store::{display_name, io_words, read_limited, write_atomic};

/// Session files larger than this are not listed (the session crate refuses
/// 128 MiB; listing reads every file, so keep it lighter).
pub const LIST_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Stopped sessions kept in memory when autosave is off.
pub const UNSAVED_KEEP: usize = 8;

/// True for ids that are safe file names: ASCII letters, digits, `-`, `_`
/// and `.`, not starting with a dot, at most 120 characters.
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 120
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// How a session started; OBS only stops the sessions it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    User,
    Obs,
}

/// A session that just stopped, to be saved outside any lock.
#[derive(Debug)]
pub struct Stopped {
    pub session: Session,
    /// Where to save it; `None` when autosave is off.
    pub path: Option<PathBuf>,
}

/// The in-memory part: the running session and unsaved stopped ones. Every
/// method is quick and does no disk IO, so the pipeline thread never waits
/// on a file while it records a callout.
#[derive(Debug)]
pub struct SessionManager {
    folder: PathBuf,
    autosave: bool,
    active: Option<(SessionLog, Origin)>,
    unsaved: Vec<Session>,
}

impl SessionManager {
    pub fn new(folder: PathBuf, autosave: bool) -> SessionManager {
        SessionManager {
            folder,
            autosave,
            active: None,
            unsaved: Vec::new(),
        }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn set_folder(&mut self, folder: PathBuf, autosave: bool) {
        self.folder = folder;
        self.autosave = autosave;
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn origin(&self) -> Option<Origin> {
        self.active.as_ref().map(|(_, o)| *o)
    }

    /// Starts a session unless one is running. Returns true when it started.
    /// `taken` says whether an id is already used by a saved file.
    pub fn start(
        &mut self,
        started_unix_ms: u64,
        t_ms: u64,
        origin: Origin,
        taken: &dyn Fn(&str) -> bool,
    ) -> bool {
        if self.active.is_some() {
            return false;
        }
        let base = Session::default_id(started_unix_ms);
        let mut id = base.clone();
        // Two sessions started within one second get distinct ids.
        let mut n = 2;
        while taken(&id) || self.unsaved.iter().any(|s| s.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.active = Some((SessionLog::start(id, started_unix_ms, t_ms), origin));
        true
    }

    /// Stops the running session and hands it over for saving.
    pub fn stop_take(&mut self, ended_unix_ms: u64, t_ms: u64) -> Option<Stopped> {
        let (mut log, _) = self.active.take()?;
        log.stop(t_ms, ended_unix_ms);
        let session = log.into_session();
        if !self.autosave {
            self.keep_unsaved(session.clone());
            return Some(Stopped {
                session,
                path: None,
            });
        }
        let path = self.folder.join(format!("{}.json", session.id));
        Some(Stopped {
            session,
            path: Some(path),
        })
    }

    /// Keeps a stopped session in memory (autosave off, or a failed save).
    pub fn keep_unsaved(&mut self, session: Session) {
        self.unsaved.retain(|s| s.id != session.id);
        self.unsaved.insert(0, session);
        self.unsaved.truncate(UNSAVED_KEEP);
    }

    /// Records a displayed callout (only while a session runs).
    pub fn on_callout(&mut self, c: &Callout) {
        if let Some((log, _)) = self.active.as_mut() {
            log.add_callout(c);
        }
    }

    pub fn add_marker(&mut self, t_ms: u64, name: Option<&str>) -> Result<usize, String> {
        match self.active.as_mut() {
            Some((log, _)) => log
                .add_marker(t_ms, name)
                .map_err(|e| sentence(&e.to_string())),
            None => Err("Start a session log first.".into()),
        }
    }

    pub fn record_event(&mut self, t_ms: u64, event: RecordEvent) {
        if let Some((log, _)) = self.active.as_mut() {
            log.record_event(t_ms, event);
        }
    }

    pub fn clock_sample(&mut self, t_ms: u64, duration_ms: u64, paused: bool) {
        if let Some((log, _)) = self.active.as_mut() {
            log.clock_sample(t_ms, duration_ms, paused);
        }
    }

    /// `Status.session`: the running session, or `None`.
    pub fn status(&self) -> Option<SessionStatus> {
        self.active.as_ref().map(|(log, _)| log.status())
    }

    /// `AppState.session`: the running session, or `None`.
    pub fn state(&self) -> Option<SessionState> {
        self.active.as_ref().map(|(log, _)| log.state())
    }

    /// Unsaved stopped sessions.
    pub fn memory_summaries(&self, now_ms: u64) -> Vec<(u64, SessionSummary)> {
        self.unsaved
            .iter()
            .map(|s| (s.started_unix_ms, s.summary(now_ms)))
            .collect()
    }

    /// The running or an unsaved session with this id.
    pub fn memory_get(&self, id: &str) -> Option<Session> {
        if let Some((log, _)) = self.active.as_ref().filter(|(l, _)| l.id() == id) {
            return Some(log.session().clone());
        }
        self.unsaved.iter().find(|s| s.id == id).cloned()
    }

    /// Renames a marker of the running or an unsaved session; `None` when
    /// the session is not in memory.
    pub fn memory_rename(
        &mut self,
        id: &str,
        index: usize,
        name: &str,
    ) -> Option<Result<Session, String>> {
        if let Some((log, _)) = self.active.as_mut().filter(|(l, _)| l.id() == id) {
            return Some(
                log.rename_marker(index, name)
                    .map(|_| log.session().clone())
                    .map_err(|e| sentence(&e.to_string())),
            );
        }
        let s = self.unsaved.iter_mut().find(|s| s.id == id)?;
        Some(
            s.rename_marker(index, name)
                .map(|_| s.clone())
                .map_err(|e| sentence(&e.to_string())),
        )
    }

    /// Forgets an unsaved session. Refuses the running one.
    pub fn memory_delete(&mut self, id: &str) -> Result<bool, String> {
        if self.active.as_ref().is_some_and(|(l, _)| l.id() == id) {
            return Err("Stop the session log before deleting it.".into());
        }
        let before = self.unsaved.len();
        self.unsaved.retain(|s| s.id != id);
        Ok(self.unsaved.len() != before)
    }
}

impl Stopped {
    /// Saves the session when autosave is on. Returns the path written.
    pub fn save(&self) -> Result<Option<PathBuf>, String> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        match self.session.save(path) {
            Ok(()) => Ok(Some(path.clone())),
            Err(e) => {
                tracing::warn!("session log could not be saved");
                let folder = path.parent().unwrap_or(path);
                Err(format!(
                    "The session log could not be saved in {}: {}. It is kept until the app quits.",
                    folder.display(),
                    e
                ))
            }
        }
    }
}

/// `<folder>/<id>.json` for a safe id.
pub fn file_for(folder: &Path, id: &str) -> Result<PathBuf, String> {
    if !is_safe_id(id) {
        return Err("That session does not exist.".into());
    }
    Ok(folder.join(format!("{id}.json")))
}

/// Saved sessions in a folder (files that are not session logs are skipped).
pub fn list_folder(folder: &Path, now_ms: u64) -> Vec<(u64, SessionSummary)> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(folder) {
        for e in entries.flatten() {
            let path = e.path();
            let Some(stem) = session_stem(&path) else {
                continue;
            };
            if let Some(s) = load_small(&path) {
                if s.id == stem {
                    out.push((s.started_unix_ms, s.summary(now_ms)));
                }
            }
        }
    }
    out
}

/// Newest first; ids already seen (memory first) are not repeated.
pub fn merge_summaries(
    memory: Vec<(u64, SessionSummary)>,
    disk: Vec<(u64, SessionSummary)>,
) -> Vec<SessionSummary> {
    let mut all = memory;
    for d in disk {
        if !all.iter().any(|(_, s)| s.id == d.1.id) {
            all.push(d);
        }
    }
    all.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.id.cmp(&a.1.id)));
    all.into_iter().map(|(_, s)| s).collect()
}

/// Loads a saved session.
pub fn load_from(folder: &Path, id: &str) -> Result<Session, String> {
    let path = file_for(folder, id)?;
    if !path.exists() {
        return Err("That session does not exist.".into());
    }
    Session::load(&path).map_err(|e| {
        format!(
            "Could not read the session {}: {}",
            display_name(&path),
            sentence(&e.to_string())
        )
    })
}

/// Renames a marker in a saved session and saves it again.
pub fn rename_in(folder: &Path, id: &str, index: usize, name: &str) -> Result<Session, String> {
    let path = file_for(folder, id)?;
    let mut s = load_from(folder, id)?;
    s.rename_marker(index, name)
        .map_err(|e| sentence(&e.to_string()))?;
    s.save(&path).map_err(|e| {
        format!(
            "Could not save the session {}: {}",
            display_name(&path),
            sentence(&e.to_string())
        )
    })?;
    Ok(s)
}

/// Deletes a saved session file. Returns false when there is none.
pub fn delete_from(folder: &Path, id: &str) -> Result<bool, String> {
    let path = file_for(folder, id)?;
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path)
        .map(|_| true)
        .map_err(|e| format!("Could not delete {}: {}", display_name(&path), io_words(&e)))
}

/// Writes an export to `path`. Returns the warnings (UI text; they can name
/// chapter markers, so they are never logged).
pub fn export_session(
    session: &Session,
    format: ExportFormat,
    path: &Path,
    linger_ms: u64,
) -> Result<Vec<String>, String> {
    let opts = ExportOptions {
        linger_ms,
        ..ExportOptions::default()
    };
    let export = session.export(format, &opts);
    write_atomic(path, export.text.as_bytes())
        .map_err(|e| format!("Could not save {}: {}", display_name(path), io_words(&e)))?;
    Ok(export.warnings.iter().map(|w| w.message()).collect())
}

/// The sessions as the shell uses them: the in-memory manager behind a
/// short lock, file work outside it.
#[derive(Debug)]
pub struct Sessions {
    inner: parking_lot::Mutex<SessionManager>,
}

impl Sessions {
    pub fn new(folder: PathBuf, autosave: bool) -> Sessions {
        Sessions {
            inner: parking_lot::Mutex::new(SessionManager::new(folder, autosave)),
        }
    }

    /// The in-memory manager (keep the guard short).
    pub fn lock(&self) -> parking_lot::MutexGuard<'_, SessionManager> {
        self.inner.lock()
    }

    pub fn folder(&self) -> PathBuf {
        self.inner.lock().folder().to_path_buf()
    }

    pub fn start(&self, started_unix_ms: u64, t_ms: u64, origin: Origin) -> bool {
        let folder = self.folder();
        let taken = |id: &str| folder.join(format!("{id}.json")).exists();
        self.inner
            .lock()
            .start(started_unix_ms, t_ms, origin, &taken)
    }

    /// Stops and saves. `Ok(None)` when nothing ran or autosave is off.
    pub fn stop(&self, ended_unix_ms: u64, t_ms: u64) -> Result<Option<PathBuf>, String> {
        let Some(stopped) = self.inner.lock().stop_take(ended_unix_ms, t_ms) else {
            return Ok(None);
        };
        match stopped.save() {
            Ok(p) => Ok(p),
            Err(e) => {
                self.inner.lock().keep_unsaved(stopped.session);
                Err(e)
            }
        }
    }

    pub fn list(&self, now_ms: u64) -> Vec<SessionSummary> {
        let (folder, memory) = {
            let m = self.inner.lock();
            (m.folder().to_path_buf(), m.memory_summaries(now_ms))
        };
        merge_summaries(memory, list_folder(&folder, now_ms))
    }

    pub fn get(&self, id: &str) -> Result<Session, String> {
        let folder = {
            let m = self.inner.lock();
            if let Some(s) = m.memory_get(id) {
                return Ok(s);
            }
            m.folder().to_path_buf()
        };
        load_from(&folder, id)
    }

    pub fn rename_marker(&self, id: &str, index: usize, name: &str) -> Result<Session, String> {
        let folder = {
            let mut m = self.inner.lock();
            if let Some(r) = m.memory_rename(id, index, name) {
                return r;
            }
            m.folder().to_path_buf()
        };
        rename_in(&folder, id, index, name)
    }

    pub fn export(
        &self,
        id: &str,
        format: ExportFormat,
        path: &Path,
        linger_ms: u64,
    ) -> Result<Vec<String>, String> {
        let s = self.get(id)?;
        export_session(&s, format, path, linger_ms)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let (in_memory, folder) = {
            let mut m = self.inner.lock();
            (m.memory_delete(id)?, m.folder().to_path_buf())
        };
        let on_disk = delete_from(&folder, id)?;
        if in_memory || on_disk {
            Ok(())
        } else {
            Err("That session does not exist.".into())
        }
    }
}

/// `<id>` for `<id>.json` files with a safe id.
fn session_stem(path: &Path) -> Option<String> {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    is_safe_id(stem).then(|| stem.to_string())
}

fn load_small(path: &Path) -> Option<Session> {
    let bytes = read_limited(path, LIST_MAX_BYTES).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    Session::from_json(text).ok()
}

/// Capitalizes an error message from a library into a sentence.
pub fn sentence(s: &str) -> String {
    let mut c = s.chars();
    let mut out = match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    };
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::{CalloutKind, Chip, ChipKind};

    fn callout(id: u64, t: u64) -> Callout {
        Callout {
            id,
            kind: CalloutKind::Combo,
            steps: vec![vec![
                Chip::new("Ctrl", "Ctrl", ChipKind::Modifier),
                Chip::new("B", "B", ChipKind::Key),
            ]],
            text: None,
            count: 1,
            caption: Some("Fixture Command One".into()),
            context: None,
            profile: Some("solidworks".into()),
            t_ms: t,
            updated_ms: t,
        }
    }

    const T0: u64 = 1_790_000_000_000;

    fn mgr(autosave: bool) -> (tempfile::TempDir, Sessions) {
        let d = tempfile::tempdir().expect("dir");
        let m = Sessions::new(d.path().join("Sessions"), autosave);
        (d, m)
    }

    #[test]
    fn ids_are_plain_file_names() {
        assert!(is_safe_id("2026-10-01T14-03-22Z"));
        assert!(is_safe_id("2026-10-01T14-03-22Z-2"));
        for bad in ["", "..", "../x", "a/b", "a\\b", ".hidden", "C:x", "a b"] {
            assert!(!is_safe_id(bad), "{bad}");
        }
        assert!(!is_safe_id(&"a".repeat(121)));
    }

    #[test]
    fn start_record_stop_save_list_get() {
        let (_d, m) = mgr(true);
        assert!(m.lock().status().is_none());
        assert!(m.start(T0, 1000, Origin::User));
        assert!(!m.start(T0, 1000, Origin::User), "one at a time");
        m.lock().on_callout(&callout(1, 1100));
        m.lock().on_callout(&callout(2, 1200));
        assert_eq!(
            m.lock().add_marker(1300, Some("Sketch")).expect("marker"),
            0
        );
        let st = m.lock().status().expect("status");
        assert!(st.active);
        assert_eq!((st.callouts, st.markers), (2, 1));
        assert_eq!(m.lock().state().expect("state").started_ms, 1000);
        let path = m.stop(T0 + 5000, 6000).expect("saved").expect("path");
        assert!(path.exists());
        assert!(m.lock().status().is_none());
        let list = m.list(9000);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].callouts, 2);
        assert_eq!(list[0].duration_ms, 5000);
        let s = m.get(&list[0].id).expect("get");
        assert_eq!(s.markers[0].name, "Sketch");
        assert!(
            m.lock().add_marker(7000, None).is_err(),
            "no session running"
        );
        assert_eq!(m.stop(T0, 7000).expect("nothing running"), None);
    }

    #[test]
    fn rename_export_delete() {
        let (d, m) = mgr(true);
        m.start(T0, 0, Origin::User);
        m.lock().on_callout(&callout(1, 100));
        m.lock().add_marker(200, None).expect("m");
        m.stop(T0 + 1000, 1000).expect("stop");
        let id = m.list(2000)[0].id.clone();
        let s = m.rename_marker(&id, 0, "Extrude").expect("rename");
        assert_eq!(s.markers[0].name, "Extrude");
        assert_eq!(m.get(&id).expect("g").markers[0].name, "Extrude");
        assert!(m.rename_marker(&id, 5, "x").is_err());
        let out = d.path().join("exports").join("video.srt");
        let warnings = m
            .export(&id, ExportFormat::Srt, &out, 2200)
            .expect("export");
        let text = fs::read_to_string(&out).expect("srt");
        assert!(text.contains("Ctrl + B"), "{text}");
        assert!(warnings.iter().all(|w| !w.is_empty()));
        let md = d.path().join("shortcuts.md");
        m.export(&id, ExportFormat::Md, &md, 2200).expect("md");
        assert!(fs::read_to_string(&md)
            .expect("md")
            .contains("Fixture Command One"));
        m.delete(&id).expect("deleted");
        assert!(m.list(3000).is_empty());
        assert!(m.delete(&id).is_err());
        assert!(m.get("../settings").is_err());
        assert!(m.delete("..\\x").is_err());
    }

    #[test]
    fn autosave_off_keeps_the_session_in_memory() {
        let (_d, m) = mgr(false);
        m.start(T0, 0, Origin::Obs);
        assert_eq!(m.lock().origin(), Some(Origin::Obs));
        m.lock().on_callout(&callout(1, 10));
        assert_eq!(m.stop(T0 + 100, 100).expect("ok"), None);
        assert!(!m.folder().exists(), "nothing written");
        let list = m.list(200);
        assert_eq!(list.len(), 1);
        let id = list[0].id.clone();
        assert_eq!(m.get(&id).expect("g").callouts.len(), 1);
        m.rename_marker(&id, 0, "x").expect_err("no markers");
        m.delete(&id).expect("delete");
        assert!(m.list(300).is_empty());
    }

    #[test]
    fn the_running_session_is_readable_but_not_deletable() {
        let (_d, m) = mgr(true);
        m.start(T0, 0, Origin::User);
        m.lock().add_marker(10, None).expect("m");
        let id = m.lock().status().expect("s").id;
        assert_eq!(m.get(&id).expect("g").markers.len(), 1);
        m.rename_marker(&id, 0, "Intro").expect("rename live");
        assert!(m.delete(&id).is_err());
    }

    #[test]
    fn ids_are_unique_within_a_second() {
        let (_d, m) = mgr(true);
        m.start(T0, 0, Origin::User);
        m.stop(T0 + 10, 10).expect("s");
        m.start(T0, 20, Origin::User);
        m.stop(T0 + 30, 30).expect("s");
        let list = m.list(40);
        assert_eq!(list.len(), 2);
        assert_ne!(list[0].id, list[1].id);
    }

    #[test]
    fn other_files_are_ignored() {
        let (_d, m) = mgr(true);
        fs::create_dir_all(m.folder()).expect("d");
        fs::write(m.folder().join("notes.json"), "{}").expect("w");
        fs::write(m.folder().join("readme.txt"), "x").expect("w");
        assert!(m.list(0).is_empty());
    }

    #[test]
    fn a_failed_save_keeps_the_session() {
        let d = tempfile::tempdir().expect("d");
        let blocker = d.path().join("file");
        fs::write(&blocker, "x").expect("w");
        // The folder path runs through an existing file, so the save fails.
        let m = Sessions::new(blocker.join("Sessions"), true);
        m.start(T0, 0, Origin::User);
        let e = m.stop(T0 + 10, 10).expect_err("cannot save");
        assert!(e.starts_with("The session log could not be saved"), "{e}");
        assert_eq!(m.list(20).len(), 1);
    }

    #[test]
    fn sentences() {
        assert_eq!(
            sentence("a chapter name cannot be empty"),
            "A chapter name cannot be empty."
        );
        assert_eq!(sentence("Done."), "Done.");
    }
}
