//! Lifecycle logging to a small rotating file in the app log directory
//! (`keystage.log`, then `keystage.1.log` and `keystage.2.log`).
//!
//! Privacy (ARCHITECTURE.md section 8, rule 5): the log holds lifecycle
//! events only. The filter is fixed (no `RUST_LOG`): this app's crates at
//! info, everything else at error, and the `log` crate bridge is never
//! installed, so no dependency can route key names or text into the file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Bytes per file before it rotates.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Files kept, the current one included.
pub const KEEP_FILES: usize = 3;
/// The current file's name.
pub const FILE_NAME: &str = "keystage.log";

/// The fixed filter: lifecycle from this app, errors from the rest.
pub const FILTER: &str = "error,idea_keystage_lib=info,idea_keystage=info,keystage_server=info,keystage_obs=info,keystage_session=info,keystage_core=info";

/// A file writer that rotates by size.
#[derive(Debug)]
pub struct RotatingFile {
    dir: PathBuf,
    max_bytes: u64,
    keep: usize,
    file: Option<File>,
    written: u64,
}

impl RotatingFile {
    /// Opens (or creates) `dir/keystage.log` for appending.
    pub fn open(dir: &Path, max_bytes: u64, keep: usize) -> io::Result<RotatingFile> {
        fs::create_dir_all(dir)?;
        let mut rf = RotatingFile {
            dir: dir.to_path_buf(),
            max_bytes: max_bytes.max(1024),
            keep: keep.max(1),
            file: None,
            written: 0,
        };
        rf.open_current()?;
        Ok(rf)
    }

    fn name(&self, index: usize) -> PathBuf {
        if index == 0 {
            self.dir.join(FILE_NAME)
        } else {
            self.dir.join(format!("keystage.{index}.log"))
        }
    }

    fn open_current(&mut self) -> io::Result<()> {
        let path = self.name(0);
        let f = OpenOptions::new().create(true).append(true).open(&path)?;
        self.written = f.metadata().map(|m| m.len()).unwrap_or(0);
        self.file = Some(f);
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        let last = self.name(self.keep - 1);
        if self.keep == 1 {
            let _ = fs::remove_file(&last);
        } else {
            let _ = fs::remove_file(&last);
            for i in (0..self.keep - 1).rev() {
                let from = self.name(i);
                if from.exists() {
                    fs::rename(&from, self.name(i + 1))?;
                }
            }
        }
        self.open_current()
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written > 0 && self.written + buf.len() as u64 > self.max_bytes {
            self.rotate()?;
        }
        let f = match self.file.as_mut() {
            Some(f) => f,
            None => {
                self.open_current()?;
                self.file
                    .as_mut()
                    .ok_or_else(|| io::Error::other("log file not open"))?
            }
        };
        let n = f.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}

/// Installs the global subscriber writing to `dir`. Safe to call once; a
/// second call (or a failure) leaves logging as it was.
pub fn init(dir: &Path) -> Result<(), String> {
    let file = RotatingFile::open(dir, MAX_FILE_BYTES, KEEP_FILES)
        .map_err(|e| format!("could not open the log file: {e}"))?;
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_env_filter(tracing_subscriber::EnvFilter::new(FILTER))
        .with_writer(Mutex::new(file))
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|_| "a logger was already installed".to_string())?;
    install_panic_hook();
    Ok(())
}

/// Logs where a panic happened, never its message (which could quote data).
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        match info.location() {
            Some(l) => tracing::error!(thread = name, file = l.file(), line = l.line(), "panic"),
            None => tracing::error!(thread = name, "panic"),
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .expect("dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn rotates_and_keeps_three_files() {
        let dir = tempfile::tempdir().expect("dir");
        let mut rf = RotatingFile::open(dir.path(), 1024, 3).expect("open");
        let line = [b'x'; 100];
        for _ in 0..60 {
            rf.write_all(&line).expect("write");
        }
        rf.flush().expect("flush");
        assert_eq!(
            files(dir.path()),
            vec!["keystage.1.log", "keystage.2.log", "keystage.log"]
        );
        for f in files(dir.path()) {
            let len = fs::metadata(dir.path().join(f)).expect("m").len();
            assert!(len <= 1024, "{len}");
        }
    }

    #[test]
    fn appends_across_opens() {
        let dir = tempfile::tempdir().expect("dir");
        {
            let mut rf = RotatingFile::open(dir.path(), 4096, 2).expect("open");
            rf.write_all(b"one\n").expect("w");
        }
        {
            let mut rf = RotatingFile::open(dir.path(), 4096, 2).expect("open");
            rf.write_all(b"two\n").expect("w");
        }
        let text = fs::read_to_string(dir.path().join(FILE_NAME)).expect("r");
        assert_eq!(text, "one\ntwo\n");
    }

    #[test]
    fn single_file_mode_truncates_by_rotation() {
        let dir = tempfile::tempdir().expect("dir");
        let mut rf = RotatingFile::open(dir.path(), 1024, 1).expect("open");
        for _ in 0..30 {
            rf.write_all(&[b'y'; 100]).expect("w");
        }
        assert_eq!(files(dir.path()), vec![FILE_NAME.to_string()]);
    }

    #[test]
    fn the_filter_parses_and_is_strict() {
        let f = tracing_subscriber::EnvFilter::new(FILTER);
        let s = f.to_string();
        assert!(s.contains("idea_keystage_lib=info"));
        assert!(FILTER.starts_with("error,"));
    }
}
