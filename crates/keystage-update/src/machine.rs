//! When to check, what a check leads to, and when an update may install.
//!
//! [`Updater`] holds no IO of its own: everything that touches the network,
//! the disk or Windows goes through [`Env`], so the whole decision path is
//! tested with a fake. The app shell runs it on one worker thread.
//!
//! * The first automatic check is [`START_DELAY`] after start, then every
//!   [`CHECK_INTERVAL`]; a failed check is retried after [`RETRY_DELAY`].
//!   With automatic checks off nothing is checked unless the user asks.
//! * A newer release is downloaded and verified ([`crate::fetch`]), then
//!   installed at once unless a Session Log or an OBS recording is running
//!   ([`Env::busy`]); then it waits, and installs on the first tick after
//!   the recording stops.
//! * A copy that was not installed by the installer (the portable exe, a
//!   development build) only reports that a newer version exists.

use std::path::{Path, PathBuf};
use std::time::Duration;

use keystage_core::ipc::{UpdateState, UpdateStatus};

use crate::release::{Plan, Release, Source, Verdict};
use crate::version::Version;
use crate::UpdateError;

/// The first automatic check after start.
pub const START_DELAY: Duration = Duration::from_secs(20);
/// Between automatic checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// After a check that failed (no network at sign-in, GitHub unreachable).
pub const RETRY_DELAY: Duration = Duration::from_secs(30 * 60);
/// The worker never sleeps longer than this, so a recording that stopped or
/// a switch that changed is noticed soon.
pub const MAX_WAIT: Duration = Duration::from_secs(5);

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// What the updater needs from the app.
pub trait Env {
    /// The latest release (`GET .../releases/latest`).
    fn latest(&mut self, source: &Source) -> Result<Release, UpdateError>;
    /// Downloads and verifies the installer; returns where it is.
    fn download(&mut self, plan: &Plan) -> Result<PathBuf, UpdateError>;
    /// True while a Session Log or an OBS recording runs.
    fn busy(&self) -> bool;
    /// True when this copy was installed by the installer and may update itself.
    fn can_install(&self) -> bool;
    /// Verifies the installer again and starts it. On `Ok` the app exits.
    fn install(&mut self, version: &Version, installer: &Path) -> Result<(), UpdateError>;
    /// The status changed (the app publishes it).
    fn changed(&mut self, status: &UpdateStatus);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available(Version),
    Downloading(Version),
    Ready(Version, PathBuf),
    Installing(Version),
    Updated(Version),
    Failed(UpdateError),
}

/// What one tick did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tick {
    /// Nothing to do now; tick again within the returned wait.
    Wait(Duration),
    /// The installer was started: quit the app now.
    Quit,
}

pub struct Updater {
    current: Version,
    source: Source,
    next_check_ms: u64,
    phase: Phase,
    checked: Option<String>,
}

impl Updater {
    /// `now_ms` is a monotonic clock; `startup` is what the last run left
    /// behind (see [`crate::install::Marker`]).
    pub fn new(current: Version, source: Source, now_ms: u64, startup: Option<Phase>) -> Updater {
        Updater {
            current,
            source,
            next_check_ms: now_ms.saturating_add(ms(START_DELAY)),
            phase: startup.unwrap_or(Phase::Idle),
            checked: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// The status for the settings window and the tray.
    pub fn status(&self, auto: bool) -> UpdateStatus {
        let cur = self.current.to_string();
        let (state, message, version) = match &self.phase {
            Phase::Idle if !auto => (
                UpdateState::Off,
                "Automatic updates are off. Use Check now to look for a new version.".to_string(),
                None,
            ),
            Phase::Idle => (
                UpdateState::Idle,
                "IDEA Keystage checks for updates shortly after it starts.".to_string(),
                None,
            ),
            Phase::Checking => (
                UpdateState::Checking,
                "Checking for updates.".to_string(),
                None,
            ),
            Phase::UpToDate => (
                UpdateState::UpToDate,
                format!("IDEA Keystage is up to date ({cur})."),
                Some(cur),
            ),
            Phase::Available(v) => (
                UpdateState::Available,
                format!(
                    "Version {v} is available. This copy was not set up by the installer, so it does not update itself; download the installer from the releases page."
                ),
                Some(v.to_string()),
            ),
            Phase::Downloading(v) => (
                UpdateState::Downloading,
                format!("Downloading version {v}."),
                Some(v.to_string()),
            ),
            Phase::Ready(v, _) => (
                UpdateState::Ready,
                format!("Update {v} is downloaded and installs when recording stops."),
                Some(v.to_string()),
            ),
            Phase::Installing(v) => (
                UpdateState::Installing,
                format!("Installing version {v}. IDEA Keystage restarts by itself."),
                Some(v.to_string()),
            ),
            Phase::Updated(v) => (
                UpdateState::Updated,
                format!("Updated to {v}."),
                Some(v.to_string()),
            ),
            Phase::Failed(e) => (UpdateState::Error, e.message(), None),
        };
        UpdateStatus {
            state,
            message,
            version,
            checked: self.checked.clone(),
        }
    }

    fn set(&mut self, phase: Phase, env: &mut dyn Env, auto: bool) {
        self.phase = phase;
        let s = self.status(auto);
        env.changed(&s);
    }

    fn try_install(&mut self, env: &mut dyn Env, auto: bool) -> Option<Tick> {
        let Phase::Ready(v, path) = self.phase.clone() else {
            return None;
        };
        if env.busy() {
            return None;
        }
        self.set(Phase::Installing(v.clone()), env, auto);
        match env.install(&v, &path) {
            Ok(()) => Some(Tick::Quit),
            Err(e) => {
                self.set(Phase::Failed(e), env, auto);
                None
            }
        }
    }

    /// One step. `manual` is a Check now from the user; `auto` is the
    /// "Check for updates automatically" switch; `wall` is the time to show
    /// as the last check.
    pub fn tick(
        &mut self,
        env: &mut dyn Env,
        now_ms: u64,
        auto: bool,
        manual: bool,
        wall: &dyn Fn() -> String,
    ) -> Tick {
        if let Some(t) = self.try_install(env, auto) {
            return t;
        }
        let due = manual || (auto && now_ms >= self.next_check_ms);
        let waiting = matches!(
            self.phase,
            Phase::Ready(..) | Phase::Installing(_) | Phase::Checking | Phase::Downloading(_)
        );
        if due && !waiting {
            let ok = self.check(env, auto);
            self.checked = Some(wall());
            let delay = if ok { CHECK_INTERVAL } else { RETRY_DELAY };
            self.next_check_ms = now_ms.saturating_add(ms(delay));
            let s = self.status(auto);
            env.changed(&s);
            if let Some(t) = self.try_install(env, auto) {
                return t;
            }
        } else if due && manual {
            // Already downloaded: say so again.
            let s = self.status(auto);
            env.changed(&s);
        }
        let wait = if auto && !matches!(self.phase, Phase::Ready(..)) {
            Duration::from_millis(self.next_check_ms.saturating_sub(now_ms)).min(MAX_WAIT)
        } else {
            MAX_WAIT
        };
        Tick::Wait(wait)
    }

    /// Checks, and downloads a newer version. True unless something failed.
    fn check(&mut self, env: &mut dyn Env, auto: bool) -> bool {
        self.set(Phase::Checking, env, auto);
        let verdict = env
            .latest(&self.source)
            .and_then(|r| r.verdict(&self.current, &self.source));
        let plan = match verdict {
            Ok(Verdict::UpToDate { .. }) => {
                self.set(Phase::UpToDate, env, auto);
                return true;
            }
            Ok(Verdict::Newer(p)) => p,
            Err(e) => {
                self.set(Phase::Failed(e), env, auto);
                return false;
            }
        };
        if !env.can_install() {
            self.set(Phase::Available(plan.version.clone()), env, auto);
            return true;
        }
        self.set(Phase::Downloading(plan.version.clone()), env, auto);
        match env.download(&plan) {
            Ok(path) => {
                self.set(Phase::Ready(plan.version.clone(), path), env, auto);
                true
            }
            Err(e) => {
                self.set(Phase::Failed(e), env, auto);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Fake {
        release: Result<Release, UpdateError>,
        download: Result<PathBuf, UpdateError>,
        busy: Cell<bool>,
        installed: bool,
        install_result: Result<(), UpdateError>,
        latest_calls: u32,
        downloads: u32,
        installs: Vec<(String, PathBuf)>,
        seen: Vec<UpdateState>,
    }

    fn release(tag: &str) -> Release {
        let names = [
            format!(
                "IDEA-Keystage_{}_x64-setup.exe",
                tag.trim_start_matches('v')
            ),
            "SHA256SUMS.txt".to_string(),
        ];
        let assets: Vec<serde_json::Value> = names
            .iter()
            .map(|n| serde_json::json!({"name": n, "size": 10, "browser_download_url": format!("{}{tag}/{n}", crate::DOWNLOAD_PREFIX)}))
            .collect();
        Release::from_json(&serde_json::json!({"tag_name": tag, "assets": assets}).to_string())
            .expect("release")
    }

    impl Fake {
        fn new(tag: &str) -> Fake {
            Fake {
                release: Ok(release(tag)),
                download: Ok(PathBuf::from("setup.exe")),
                busy: Cell::new(false),
                installed: true,
                install_result: Ok(()),
                latest_calls: 0,
                downloads: 0,
                installs: Vec::new(),
                seen: Vec::new(),
            }
        }
    }

    impl Env for Fake {
        fn latest(&mut self, _s: &Source) -> Result<Release, UpdateError> {
            self.latest_calls += 1;
            self.release.clone()
        }
        fn download(&mut self, _p: &Plan) -> Result<PathBuf, UpdateError> {
            self.downloads += 1;
            self.download.clone()
        }
        fn busy(&self) -> bool {
            self.busy.get()
        }
        fn can_install(&self) -> bool {
            self.installed
        }
        fn install(&mut self, v: &Version, p: &Path) -> Result<(), UpdateError> {
            self.installs.push((v.to_string(), p.to_path_buf()));
            self.install_result.clone()
        }
        fn changed(&mut self, s: &UpdateStatus) {
            self.seen.push(s.state);
        }
    }

    fn updater() -> Updater {
        Updater::new(
            Version::parse("0.2.0").expect("v"),
            Source::github(),
            1_000,
            None,
        )
    }

    fn wall() -> String {
        "2026-11-04T12:00:00.000Z".into()
    }

    const START: u64 = 1_000 + 20_000;

    #[test]
    fn checks_after_the_start_delay_then_every_six_hours() {
        let mut u = updater();
        let mut env = Fake::new("v0.2.0");
        assert_eq!(
            u.tick(&mut env, 1_000, true, false, &wall),
            Tick::Wait(MAX_WAIT)
        );
        assert_eq!(
            u.tick(&mut env, START - 1, true, false, &wall),
            Tick::Wait(Duration::from_millis(1))
        );
        assert_eq!(env.latest_calls, 0, "not before the delay");
        u.tick(&mut env, START, true, false, &wall);
        assert_eq!(env.latest_calls, 1);
        assert_eq!(u.status(true).state, UpdateState::UpToDate);
        assert_eq!(
            u.status(true).message,
            "IDEA Keystage is up to date (0.2.0)."
        );
        assert_eq!(
            u.status(true).checked.as_deref(),
            Some("2026-11-04T12:00:00.000Z")
        );
        u.tick(&mut env, START + ms(CHECK_INTERVAL) - 1, true, false, &wall);
        assert_eq!(env.latest_calls, 1);
        u.tick(&mut env, START + ms(CHECK_INTERVAL), true, false, &wall);
        assert_eq!(env.latest_calls, 2);
        assert_eq!(
            env.seen[..2],
            [UpdateState::Checking, UpdateState::UpToDate]
        );
    }

    #[test]
    fn off_means_only_check_now() {
        let mut u = updater();
        let mut env = Fake::new("v0.2.0");
        assert_eq!(u.status(false).state, UpdateState::Off);
        u.tick(
            &mut env,
            START + ms(CHECK_INTERVAL) * 3,
            false,
            false,
            &wall,
        );
        assert_eq!(env.latest_calls, 0);
        u.tick(&mut env, START, false, true, &wall);
        assert_eq!(env.latest_calls, 1, "Check now works with the switch off");
    }

    #[test]
    fn a_newer_version_installs_at_once_when_nothing_records() {
        let mut u = updater();
        let mut env = Fake::new("v0.3.0");
        assert_eq!(u.tick(&mut env, START, true, false, &wall), Tick::Quit);
        assert_eq!(env.downloads, 1);
        assert_eq!(
            env.installs,
            vec![("0.3.0".into(), PathBuf::from("setup.exe"))]
        );
        assert_eq!(
            env.seen,
            vec![
                UpdateState::Checking,
                UpdateState::Downloading,
                UpdateState::Ready,
                UpdateState::Ready,
                UpdateState::Installing
            ]
        );
    }

    #[test]
    fn recording_defers_the_install_until_it_stops() {
        let mut u = updater();
        let mut env = Fake::new("v0.3.0");
        env.busy.set(true);
        assert_eq!(
            u.tick(&mut env, START, true, false, &wall),
            Tick::Wait(MAX_WAIT)
        );
        assert_eq!(
            u.status(true).message,
            "Update 0.3.0 is downloaded and installs when recording stops."
        );
        assert!(env.installs.is_empty());
        // Still recording: no install, no new check, no second download.
        let later = START + ms(CHECK_INTERVAL) * 2;
        assert_eq!(
            u.tick(&mut env, later, true, true, &wall),
            Tick::Wait(MAX_WAIT)
        );
        assert_eq!((env.latest_calls, env.downloads), (1, 1));
        assert!(env.installs.is_empty());
        env.busy.set(false);
        assert_eq!(
            u.tick(&mut env, later + 5_000, true, false, &wall),
            Tick::Quit
        );
        assert_eq!(env.installs.len(), 1);
    }

    #[test]
    fn failures_are_shown_and_retried_sooner() {
        let mut u = updater();
        let mut env = Fake::new("v0.3.0");
        env.release = Err(UpdateError::Network);
        u.tick(&mut env, START, true, false, &wall);
        assert_eq!(u.status(true).state, UpdateState::Error);
        assert_eq!(u.status(true).message, UpdateError::Network.message());
        u.tick(&mut env, START + ms(RETRY_DELAY) - 1, true, false, &wall);
        assert_eq!(env.latest_calls, 1);
        env.release = Ok(release("v0.3.0"));
        env.download = Err(UpdateError::ChecksumMismatch);
        assert_eq!(
            u.tick(&mut env, START + ms(RETRY_DELAY), true, false, &wall),
            Tick::Wait(MAX_WAIT)
        );
        assert_eq!(env.latest_calls, 2);
        assert!(env.installs.is_empty(), "a mismatch never installs");
        assert!(u.status(true).message.contains("did not match"));
        // An installer that would not start leaves the app running.
        let mut u = updater();
        let mut env = Fake::new("v0.3.0");
        env.install_result = Err(UpdateError::LaunchFailed);
        assert_eq!(
            u.tick(&mut env, START, true, false, &wall),
            Tick::Wait(MAX_WAIT)
        );
        assert_eq!(u.status(true).state, UpdateState::Error);
    }

    #[test]
    fn a_portable_copy_only_reports() {
        let mut u = updater();
        let mut env = Fake::new("v0.3.0");
        env.installed = false;
        assert_eq!(
            u.tick(&mut env, START, true, false, &wall),
            Tick::Wait(MAX_WAIT)
        );
        assert_eq!(env.downloads, 0);
        assert_eq!(u.status(true).state, UpdateState::Available);
        assert_eq!(u.status(true).version.as_deref(), Some("0.3.0"));
    }

    #[test]
    fn messages_are_plain_sentences() {
        let v = Version::parse("0.3.0").expect("v");
        for phase in [
            Phase::Idle,
            Phase::Checking,
            Phase::UpToDate,
            Phase::Available(v.clone()),
            Phase::Downloading(v.clone()),
            Phase::Ready(v.clone(), PathBuf::from("x")),
            Phase::Installing(v.clone()),
            Phase::Updated(v.clone()),
            Phase::Failed(UpdateError::Network),
        ] {
            let u = Updater::new(
                Version::parse("0.2.0").expect("v"),
                Source::github(),
                0,
                Some(phase),
            );
            for auto in [true, false] {
                let m = u.status(auto).message;
                assert!(m.ends_with('.'), "{m}");
                assert!(!m.contains('\u{2014}') && !m.contains('\u{2013}'), "{m}");
                assert!(
                    !m.contains("x\\") && !m.contains("setup.exe"),
                    "no paths: {m}"
                );
            }
        }
        let u = Updater::new(
            Version::parse("0.3.0").expect("v"),
            Source::github(),
            0,
            Some(Phase::Updated(v)),
        );
        assert_eq!(u.status(true).message, "Updated to 0.3.0.");
    }
}
