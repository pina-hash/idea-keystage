//! Automatic updates in the app (ARCHITECTURE.md section 13): one worker
//! thread, `keystage-update`, runs [`keystage_update::Updater`] with the
//! app's [`Env`]: the HTTPS client, the updates folder
//! (`%LOCALAPPDATA%\com.ideabosco.keystage\updates`), "busy" while a Session
//! Log or an OBS recording runs, and the Windows launch of the installer.
//!
//! Privacy (section 8): rule 7 allows exactly these requests; rule 5 keeps
//! the log to lifecycle lines with a fixed reason word, never a path, an
//! address or an error text.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{RecvTimeoutError, Sender};
use keystage_core::ipc::UpdateStatus;
use keystage_core::protocol::APP_VERSION;
use keystage_update::fetch::{self, Client};
use keystage_update::install::{self, Marker};
use keystage_update::sums::{self, Digest256};
use keystage_update::{Env, Phase, Plan, Release, Source, Tick, UpdateError, Updater, Version};
use parking_lot::Mutex;

use super::shell::{unix_ms, Shell};
use crate::platform;

/// What the worker can be woken for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    /// Check now (settings window, tray).
    CheckNow,
    /// The settings changed (the automatic switch).
    Settings,
}

/// The handle the shell keeps.
pub struct UpdateService {
    status: Arc<Mutex<UpdateStatus>>,
    tx: Sender<Wake>,
}

impl UpdateService {
    pub fn status(&self) -> UpdateStatus {
        self.status.lock().clone()
    }

    pub fn wake(&self, w: Wake) {
        let _ = self.tx.send(w);
    }
}

struct AppEnv {
    shell: Weak<Shell>,
    client: Client,
    dir: PathBuf,
    current: Version,
    verified: Option<Digest256>,
    status: Arc<Mutex<UpdateStatus>>,
}

impl Env for AppEnv {
    fn latest(&mut self, source: &Source) -> Result<Release, UpdateError> {
        tracing::info!("update check started");
        let r = self.client.latest(source);
        if let Err(e) = &r {
            tracing::warn!(reason = e.reason(), "update check failed");
        }
        r
    }

    fn download(&mut self, plan: &Plan) -> Result<PathBuf, UpdateError> {
        tracing::info!(version = %plan.version, "update check found newer");
        install::clean(&self.dir);
        let r = self.client.download(plan, &self.dir).and_then(|path| {
            let f = std::fs::File::open(&path).map_err(|_| UpdateError::Disk)?;
            let d = sums::digest_reader(f).map_err(|_| UpdateError::Disk)?;
            self.verified = Some(d);
            Ok(path)
        });
        match &r {
            Ok(_) => tracing::info!(version = %plan.version, "update downloaded and verified"),
            Err(e) => tracing::warn!(reason = e.reason(), "update download failed"),
        }
        r
    }

    fn busy(&self) -> bool {
        let Some(s) = self.shell.upgrade() else {
            return true;
        };
        s.recording_busy()
    }

    fn can_install(&self) -> bool {
        cfg!(windows)
            && std::env::current_exe()
                .map(|e| install::installed_copy(&e))
                .unwrap_or(false)
    }

    fn install(&mut self, version: &Version, installer: &Path) -> Result<(), UpdateError> {
        let Some(expected) = self.verified else {
            return Err(UpdateError::ChecksumMismatch);
        };
        // What starts is exactly what was checked.
        fetch::verify_file(installer, &expected).inspect_err(|e| {
            tracing::warn!(reason = e.reason(), "update not installed");
        })?;
        Marker::write(&self.dir, &self.current, version).map_err(|_| UpdateError::Disk)?;
        match platform::launch(installer, &install::installer_parameters()) {
            Ok(()) => {
                tracing::info!(version = %version, "update installer started");
                Ok(())
            }
            Err(_) => {
                let _ = std::fs::remove_file(self.dir.join(install::MARKER_FILE));
                tracing::warn!(reason = "launch_failed", "update not installed");
                Err(UpdateError::LaunchFailed)
            }
        }
    }

    fn changed(&mut self, status: &UpdateStatus) {
        let mut cur = self.status.lock();
        if *cur != *status {
            *cur = status.clone();
            drop(cur);
            if let Some(s) = self.shell.upgrade() {
                s.refresh_soon();
            }
        }
    }
}

/// Starts the worker. `dir` is the updates folder.
pub fn spawn(shell: Weak<Shell>, dir: PathBuf) -> Result<UpdateService, String> {
    let current = Version::parse(APP_VERSION).ok_or("the app version is not a version")?;
    let startup = install::startup(&dir, &current);
    match &startup {
        Some(Phase::Updated(v)) => tracing::info!(version = %v, "update installed"),
        Some(Phase::Failed(e)) => tracing::warn!(reason = e.reason(), "update did not finish"),
        _ => {}
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    let status = Arc::new(Mutex::new(UpdateStatus::default()));
    let mut updater = Updater::new(
        current.clone(),
        Source::github(),
        platform::now_ms(),
        startup,
    );
    let auto = shell
        .upgrade()
        .map(|s| s.settings().updates.auto)
        .unwrap_or(false);
    *status.lock() = updater.status(auto);
    let mut env = AppEnv {
        shell: shell.clone(),
        client: Client::new(),
        dir,
        current,
        verified: None,
        status: status.clone(),
    };
    thread::Builder::new()
        .name("keystage-update".into())
        .spawn(move || {
            let mut wait = Duration::from_secs(1);
            loop {
                let manual = match rx.recv_timeout(wait) {
                    Ok(Wake::CheckNow) => true,
                    Ok(Wake::Settings) | Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                let Some(s) = shell.upgrade() else { break };
                if s.is_quitting() {
                    break;
                }
                let auto = s.settings().updates.auto;
                drop(s);
                let wall = || keystage_session::time::iso_utc(unix_ms());
                match updater.tick(&mut env, platform::now_ms(), auto, manual, &wall) {
                    Tick::Wait(d) => wait = d.max(Duration::from_millis(200)),
                    Tick::Quit => {
                        if let Some(s) = shell.upgrade() {
                            s.quit();
                        }
                        break;
                    }
                }
                let st = updater.status(auto);
                env.changed(&st);
            }
        })
        .map_err(|e| format!("the update thread did not start: {e}"))?;
    Ok(UpdateService { status, tx })
}
