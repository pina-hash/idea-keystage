//! Starting the installer, and what the next run learns from it.
//!
//! The installer is the Tauri NSIS bundle (`installMode: currentUser`, so no
//! administrator prompt). Its switches were read from the template that
//! tauri-bundler 2.10.1 (the bundler of `@tauri-apps/cli` 2.12.1) compiles,
//! `src/bundle/windows/nsis/installer.nsi` (docs/DECISIONS.md item 9):
//!
//! * `/P` passive: no pages, a progress bar only; the running app is closed
//!   through the Restart Manager without asking (`CheckIfAppIsRunning`).
//! * `/UPDATE`: keep the old installation in place (no uninstall step) and
//!   do not create a desktop shortcut.
//! * `/R`: in passive or silent mode, start the app again after a successful
//!   install (`.onInstSuccess`), with the text after `/ARGS` as its
//!   arguments.
//!
//! These are the switches the official `tauri-plugin-updater` 2.14.1 passes
//! in its default passive mode (`/P /UPDATE /R /ARGS`).
//!
//! Before starting the installer the app writes a [`Marker`]; the next run
//! reads it to say "Updated to 0.3.0" or that the update did not finish.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::machine::Phase;
use crate::version::Version;
use crate::UpdateError;

/// The argument the relaunched app gets: it starts in the tray, like the
/// autostart entry, and knows it was just updated.
pub const UPDATED_ARG: &str = "--updated";

/// The installer's command line.
pub fn installer_args() -> Vec<&'static str> {
    vec!["/P", "/UPDATE", "/R", "/ARGS", UPDATED_ARG]
}

/// The same, as one parameter string (`ShellExecuteW`).
pub fn installer_parameters() -> String {
    installer_args().join(" ")
}

/// The marker file in the updates folder.
pub const MARKER_FILE: &str = "pending.json";

/// What the app was doing when it started an installer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub from: String,
    pub to: String,
}

impl Marker {
    pub fn write(dir: &Path, from: &Version, to: &Version) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let m = Marker {
            from: from.to_string(),
            to: to.to_string(),
        };
        let text = serde_json::to_string(&m).map_err(io::Error::other)?;
        let tmp = dir.join(format!("{MARKER_FILE}.tmp"));
        fs::write(&tmp, text)?;
        fs::rename(&tmp, dir.join(MARKER_FILE))
    }

    pub fn read(dir: &Path) -> Option<Marker> {
        let text = fs::read_to_string(dir.join(MARKER_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

/// Reads and clears what the last run left in `dir`, and removes old
/// downloads. Returns the phase this run starts in, if any.
pub fn startup(dir: &Path, current: &Version) -> Option<Phase> {
    let marker = Marker::read(dir);
    clean(dir);
    let m = marker?;
    let to = Version::parse(&m.to)?;
    if *current >= to {
        Some(Phase::Updated(current.clone()))
    } else {
        Some(Phase::Failed(UpdateError::DidNotFinish(to.to_string())))
    }
}

/// Removes the marker and every downloaded installer.
pub fn clean(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let ours = name == MARKER_FILE
            || name.ends_with(".part")
            || name.ends_with(".tmp")
            || (name.starts_with("IDEA-Keystage_") && name.ends_with("-setup.exe"));
        if ours {
            let _ = fs::remove_file(e.path());
        }
    }
}

/// True when `exe` sits in a folder the installer made (it writes
/// `uninstall.exe` beside the app). The portable exe and development
/// builds have none, and do not update themselves.
pub fn installed_copy(exe: &Path) -> bool {
    exe.parent()
        .map(|d| d.join("uninstall.exe").is_file())
        .unwrap_or(false)
}

/// Where a verified installer for `version` is kept.
pub fn installer_path(dir: &Path, version: &Version) -> PathBuf {
    dir.join(crate::release::installer_name(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).expect("v")
    }

    #[test]
    fn the_switches_are_the_official_updaters_passive_set() {
        assert_eq!(installer_parameters(), "/P /UPDATE /R /ARGS --updated");
        // `/ARGS` must come last: NSIS `GetOptions` reads the rest of the
        // command line up to the next `/` option as the app's arguments.
        assert_eq!(installer_args().last(), Some(&UPDATED_ARG));
        assert!(!UPDATED_ARG.contains('/') && !UPDATED_ARG.contains(' '));
    }

    #[test]
    fn the_next_run_learns_what_happened() {
        let d = tempfile::tempdir().expect("dir");
        assert_eq!(startup(d.path(), &v("0.2.0")), None);
        Marker::write(d.path(), &v("0.2.0"), &v("0.3.0")).expect("write");
        fs::write(d.path().join("IDEA-Keystage_0.3.0_x64-setup.exe"), b"x").expect("w");
        fs::write(d.path().join("keep.txt"), b"x").expect("w");
        assert_eq!(
            startup(d.path(), &v("0.3.0")),
            Some(Phase::Updated(v("0.3.0")))
        );
        assert!(!d.path().join(MARKER_FILE).exists(), "read once");
        assert!(!d.path().join("IDEA-Keystage_0.3.0_x64-setup.exe").exists());
        assert!(
            d.path().join("keep.txt").exists(),
            "only our files are removed"
        );
        assert_eq!(startup(d.path(), &v("0.3.0")), None);

        Marker::write(d.path(), &v("0.2.0"), &v("0.3.0")).expect("write");
        assert_eq!(
            startup(d.path(), &v("0.2.0")),
            Some(Phase::Failed(UpdateError::DidNotFinish("0.3.0".into())))
        );
        fs::write(d.path().join(MARKER_FILE), b"{not json").expect("w");
        assert_eq!(startup(d.path(), &v("0.2.0")), None);
    }

    #[test]
    fn only_an_installed_copy_updates_itself() {
        let d = tempfile::tempdir().expect("dir");
        let exe = d.path().join("idea-keystage.exe");
        fs::write(&exe, b"x").expect("w");
        assert!(!installed_copy(&exe));
        fs::write(d.path().join("uninstall.exe"), b"x").expect("w");
        assert!(installed_copy(&exe));
        assert!(!installed_copy(Path::new("/")));
        assert_eq!(
            installer_path(d.path(), &v("0.3.0")),
            d.path().join("IDEA-Keystage_0.3.0_x64-setup.exe")
        );
    }
}
