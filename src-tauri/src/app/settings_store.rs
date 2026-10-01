//! The settings file (ARCHITECTURE.md section 7): JSON in the app config
//! directory, written atomically (a temporary file in the same folder, then
//! a rename over the old one), read leniently (a section that cannot be read
//! keeps its defaults), and never lost silently: a file that cannot be read,
//! or that loses a section, is copied aside before the app writes again, and
//! the user is told in plain words.
//!
//! Privacy: nothing here logs settings values. Settings issues are logged by
//! field name only (their messages can hold hotkey strings or app names).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use keystage_core::settings::SettingsIssue;
use keystage_core::Settings;

/// The settings file name in the app config directory.
pub const FILE_NAME: &str = "settings.json";
/// Larger files are not settings files.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// What loading found.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub settings: Settings,
    /// True when there was no settings file (first run).
    pub fresh: bool,
    /// A sentence for the user when something had to be reset.
    pub notice: Option<String>,
    /// Where the unreadable file was copied, when it was.
    pub backup: Option<PathBuf>,
    /// Fields that were adjusted or reset (names only, for the log).
    pub issue_fields: Vec<String>,
}

/// Reads and writes `settings.json` in one folder.
#[derive(Clone, Debug)]
pub struct SettingsStore {
    dir: PathBuf,
}

/// Top-level sections of the settings object. An issue on one of these
/// fields (no dot) means the whole section could not be read.
const SECTIONS: [&str; 13] = [
    "version",
    "first_run_done",
    "host",
    "server",
    "overlay",
    "display",
    "profiles",
    "privacy",
    "hotkeys",
    "sound",
    "obs",
    "session",
    "startup",
];

impl SettingsStore {
    pub fn new(dir: impl Into<PathBuf>) -> SettingsStore {
        SettingsStore { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }

    /// Loads the settings. Never fails: the worst case is the defaults plus
    /// a notice. `stamp` names a backup (a file-name-safe time).
    pub fn load(&self, stamp: &str) -> Loaded {
        let path = self.path();
        let bytes = match read_limited(&path, MAX_BYTES) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Loaded {
                    settings: Settings::default(),
                    fresh: true,
                    notice: None,
                    backup: None,
                    issue_fields: Vec::new(),
                };
            }
            Err(e) => {
                tracing::warn!(kind = ?e.kind(), "settings file could not be read");
                return self.reset_from(&path, stamp);
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => return self.reset_from(&path, stamp),
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(v) if v.is_object() => {}
            _ => return self.reset_from(&path, stamp),
        }
        let (settings, issues) = Settings::from_json(text);
        let lost: Vec<String> = issues
            .iter()
            .filter(|i| SECTIONS.contains(&i.field.as_str()) && i.field != "version")
            .map(|i| i.field.clone())
            .collect();
        let issue_fields = issue_fields(&issues);
        if !issue_fields.is_empty() {
            tracing::info!(fields = ?issue_fields, "settings adjusted while loading");
        }
        if lost.is_empty() {
            return Loaded {
                settings,
                fresh: false,
                notice: None,
                backup: None,
                issue_fields,
            };
        }
        let backup = backup_copy(&path, stamp);
        let kept = backup
            .as_ref()
            .and_then(|b| b.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let notice = match kept {
            Some(name) => format!(
                "Some settings could not be read and were reset: {}. The old file was kept as {name}.",
                lost.join(", ")
            ),
            None => format!(
                "Some settings could not be read and were reset: {}.",
                lost.join(", ")
            ),
        };
        Loaded {
            settings,
            fresh: false,
            notice: Some(notice),
            backup,
            issue_fields,
        }
    }

    fn reset_from(&self, path: &Path, stamp: &str) -> Loaded {
        let backup = backup_move(path, stamp);
        let notice = match backup.as_ref().and_then(|b| b.file_name()) {
            Some(name) => format!(
                "The settings file could not be read, so IDEA Keystage started with default settings. The old file was kept as {}.",
                name.to_string_lossy()
            ),
            None => "The settings file could not be read, so IDEA Keystage started with default settings.".to_string(),
        };
        tracing::warn!(backed_up = backup.is_some(), "settings reset to defaults");
        Loaded {
            settings: Settings::default(),
            fresh: false,
            notice: Some(notice),
            backup,
            issue_fields: Vec::new(),
        }
    }

    /// Writes the settings atomically.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        let json = settings.to_json_pretty();
        write_atomic(&self.path(), json.as_bytes())
    }
}

/// Field names of settings issues, for the log (never the messages).
pub fn issue_fields(issues: &[SettingsIssue]) -> Vec<String> {
    issues.iter().map(|i| i.field.clone()).collect()
}

/// Reads a whole file, refusing files larger than `limit`.
pub fn read_limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let meta = fs::metadata(path)?;
    if meta.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the file is too large",
        ));
    }
    fs::read(path)
}

/// Writes `bytes` to `path` through a temporary file in the same folder and
/// a rename, so a crash never leaves a half-written file behind. Creates the
/// folder when needed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// A free backup name next to `path`: `settings.unreadable-<stamp>.json`,
/// then `-2`, `-3` and so on.
fn backup_path(path: &Path, stamp: &str) -> PathBuf {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut n = 1u32;
    loop {
        let name = if n == 1 {
            format!("settings.unreadable-{stamp}.json")
        } else {
            format!("settings.unreadable-{stamp}-{n}.json")
        };
        let candidate = dir.join(name);
        if !candidate.exists() || n >= 1000 {
            return candidate;
        }
        n += 1;
    }
}

/// Moves an unreadable file aside (or copies it when it cannot be moved).
fn backup_move(path: &Path, stamp: &str) -> Option<PathBuf> {
    let target = backup_path(path, stamp);
    if fs::rename(path, &target).is_ok() {
        return Some(target);
    }
    backup_copy(path, stamp)
}

/// Copies a partly readable file aside; the original is replaced at the
/// next save.
fn backup_copy(path: &Path, stamp: &str) -> Option<PathBuf> {
    let target = backup_path(path, stamp);
    match fs::copy(path, &target) {
        Ok(_) => Some(target),
        Err(e) => {
            tracing::warn!(kind = ?e.kind(), "settings backup failed");
            None
        }
    }
}

/// Reads settings sent by the settings window. The stored OBS password blob
/// always wins (the window never sees or writes it). Returns the validated
/// settings and the fields that were adjusted.
pub fn merge_incoming(
    stored: &Settings,
    incoming: &serde_json::Value,
) -> Result<(Settings, Vec<SettingsIssue>), String> {
    if !incoming.is_object() {
        return Err("The settings could not be read.".into());
    }
    let (mut next, mut issues) = Settings::from_json(&incoming.to_string());
    next.obs.password_dpapi = stored.obs.password_dpapi.clone();
    issues.extend(next.validate());
    Ok((next, issues))
}

/// Writes settings for another computer: the DPAPI password blob only opens
/// on this Windows account, so it is left out.
pub fn export_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    let mut out = settings.clone();
    out.obs.password_dpapi = None;
    write_atomic(path, out.to_json_pretty().as_bytes())
        .map_err(|e| format!("Could not save {}: {}", display_name(path), io_words(&e)))
}

/// Reads a settings file chosen by the user. The current password blob and
/// the finished first run are kept.
pub fn import_settings(
    path: &Path,
    current: &Settings,
) -> Result<(Settings, Vec<SettingsIssue>), String> {
    let name = display_name(path);
    let bytes = read_limited(path, MAX_BYTES)
        .map_err(|e| format!("Could not read {name}: {}", io_words(&e)))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{name} is not a settings file."))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| format!("{name} is not a settings file."))?;
    let looks_like_settings = value
        .as_object()
        .is_some_and(|o| o.keys().any(|k| SECTIONS.contains(&k.as_str())));
    if !looks_like_settings {
        return Err(format!("{name} is not a settings file."));
    }
    let (mut next, mut issues) = Settings::from_json(text);
    next.obs.password_dpapi = current.obs.password_dpapi.clone();
    next.first_run_done = true;
    issues.extend(next.validate());
    Ok((next, issues))
}

/// The file name of a path, for messages.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Plain words for an IO error.
pub fn io_words(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::NotFound => "the file or folder does not exist".into(),
        io::ErrorKind::PermissionDenied => "Windows denied access".into(),
        io::ErrorKind::InvalidData => "the file is not valid".into(),
        io::ErrorKind::AlreadyExists => "a file with that name already exists".into(),
        _ => "an error occurred while reading or writing".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::protocol::SolidworksMode;

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::new(dir.path().join("config"));
        (dir, store)
    }

    #[test]
    fn missing_file_is_a_first_run() {
        let (_d, s) = store();
        let l = s.load("T");
        assert!(l.fresh);
        assert!(l.notice.is_none());
        assert_eq!(l.settings, Settings::default());
        assert!(!l.settings.first_run_done);
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_d, s) = store();
        let mut st = Settings {
            first_run_done: true,
            ..Settings::default()
        };
        st.profiles.solidworks_mode = SolidworksMode::On;
        st.server.port = 50000;
        s.save(&st).expect("saved");
        let l = s.load("T");
        assert!(!l.fresh);
        assert!(l.notice.is_none());
        assert_eq!(l.settings, st);
        let leftovers: Vec<_> = fs::read_dir(s.dir())
            .expect("dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec![FILE_NAME.to_string()], "no temp file left");
    }

    #[test]
    fn missing_fields_take_defaults() {
        let (_d, s) = store();
        fs::create_dir_all(s.dir()).expect("dir");
        fs::write(
            s.path(),
            r#"{"first_run_done":true,"server":{"port":50001}}"#,
        )
        .expect("w");
        let l = s.load("T");
        assert!(l.notice.is_none());
        assert!(l.settings.first_run_done);
        assert_eq!(l.settings.server.port, 50001);
        assert_eq!(l.settings.sound, Settings::default().sound);
    }

    #[test]
    fn corrupt_file_is_moved_aside_and_defaults_load() {
        let (_d, s) = store();
        fs::create_dir_all(s.dir()).expect("dir");
        fs::write(s.path(), "{ not json").expect("w");
        let l = s.load("2026-10-01T10-00-00Z");
        assert_eq!(l.settings, Settings::default());
        let notice = l.notice.expect("a notice");
        assert!(notice.contains("could not be read"), "{notice}");
        assert!(notice.contains("settings.unreadable-2026-10-01T10-00-00Z.json"));
        let b = l.backup.expect("backup");
        assert_eq!(
            fs::read_to_string(&b).expect("backup readable"),
            "{ not json"
        );
        assert!(!s.path().exists(), "the bad file was moved");
        // A second corrupt file gets a distinct backup name.
        fs::write(s.path(), "[]").expect("w");
        let l2 = s.load("2026-10-01T10-00-00Z");
        let b2 = l2.backup.expect("backup");
        assert_ne!(b, b2);
        assert!(b2.to_string_lossy().ends_with("-2.json"));
        assert!(!notice.contains('\u{2014}'));
    }

    #[test]
    fn binary_file_is_reset() {
        let (_d, s) = store();
        fs::create_dir_all(s.dir()).expect("dir");
        fs::write(s.path(), [0xff, 0xfe, 0x00, 0x81]).expect("w");
        let l = s.load("T");
        assert!(l.notice.is_some());
        assert_eq!(l.settings, Settings::default());
    }

    #[test]
    fn a_lost_section_is_backed_up_and_reported() {
        let (_d, s) = store();
        fs::create_dir_all(s.dir()).expect("dir");
        let text = r#"{"first_run_done":true,"server":{"port":"nope"},"sound":{"enabled":true}}"#;
        fs::write(s.path(), text).expect("w");
        let l = s.load("T");
        assert!(l.settings.first_run_done);
        assert!(l.settings.sound.enabled);
        assert_eq!(l.settings.server.port, 47820);
        let notice = l.notice.expect("notice");
        assert!(notice.contains("server"), "{notice}");
        let b = l.backup.expect("backup");
        assert_eq!(fs::read_to_string(b).expect("r"), text);
        assert!(s.path().exists(), "the original stays until the next save");
    }

    #[test]
    fn a_bom_is_tolerated() {
        let (_d, s) = store();
        fs::create_dir_all(s.dir()).expect("dir");
        fs::write(s.path(), "\u{feff}{\"first_run_done\":true}").expect("w");
        let l = s.load("T");
        assert!(l.notice.is_none());
        assert!(l.settings.first_run_done);
    }

    #[test]
    fn incoming_settings_keep_the_stored_password() {
        let mut stored = Settings::default();
        stored.obs.password_dpapi = Some("QUJD".into());
        let mut sent = serde_json::to_value(Settings::default()).expect("v");
        sent["obs"]["password_dpapi"] = serde_json::Value::Null;
        sent["overlay"]["scale"] = serde_json::json!(9.0);
        let (next, issues) = merge_incoming(&stored, &sent).expect("merged");
        assert_eq!(next.obs.password_dpapi.as_deref(), Some("QUJD"));
        assert_eq!(next.overlay.scale, 4.0);
        assert!(issue_fields(&issues).contains(&"overlay.scale".to_string()));
        sent["obs"]["password_dpapi"] = serde_json::json!("ZZZZ");
        let (next, _) = merge_incoming(&stored, &sent).expect("merged");
        assert_eq!(next.obs.password_dpapi.as_deref(), Some("QUJD"));
        assert!(merge_incoming(&stored, &serde_json::json!([1])).is_err());
    }

    #[test]
    fn export_leaves_the_password_out_and_import_reads_it_back() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("out").join("keystage-settings.json");
        let mut s = Settings {
            first_run_done: true,
            ..Settings::default()
        };
        s.obs.password_dpapi = Some("QUJD".into());
        s.overlay.theme = "carbon".into();
        export_settings(&path, &s).expect("exported");
        let text = fs::read_to_string(&path).expect("r");
        assert!(!text.contains("QUJD"));
        let mut current = Settings::default();
        current.obs.password_dpapi = Some("TElWRQ==".into());
        let (imported, _) = import_settings(&path, &current).expect("imported");
        assert_eq!(imported.overlay.theme, "carbon");
        assert_eq!(imported.obs.password_dpapi.as_deref(), Some("TElWRQ=="));
        assert!(imported.first_run_done);
    }

    #[test]
    fn import_refuses_files_that_are_not_settings() {
        let dir = tempfile::tempdir().expect("dir");
        let missing = dir.path().join("nope.json");
        let e = import_settings(&missing, &Settings::default()).expect_err("missing");
        assert!(e.starts_with("Could not read nope.json"), "{e}");
        let other = dir.path().join("other.json");
        fs::write(&other, r#"{"hello":1}"#).expect("w");
        let e = import_settings(&other, &Settings::default()).expect_err("not settings");
        assert_eq!(e, "other.json is not a settings file.");
        fs::write(&other, "garbage").expect("w");
        assert!(import_settings(&other, &Settings::default()).is_err());
    }

    #[test]
    fn atomic_write_replaces_and_creates_folders() {
        let dir = tempfile::tempdir().expect("dir");
        let p = dir.path().join("a").join("b").join("f.txt");
        write_atomic(&p, b"one").expect("w1");
        write_atomic(&p, b"two").expect("w2");
        assert_eq!(fs::read(&p).expect("r"), b"two");
        assert_eq!(fs::read_dir(p.parent().expect("p")).expect("d").count(), 1);
    }

    #[test]
    fn oversized_files_are_refused() {
        let dir = tempfile::tempdir().expect("dir");
        let p = dir.path().join("big");
        fs::write(&p, vec![b' '; 64]).expect("w");
        assert!(read_limited(&p, 63).is_err());
        assert_eq!(read_limited(&p, 64).expect("ok").len(), 64);
    }
}
