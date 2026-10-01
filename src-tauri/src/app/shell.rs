//! The shell: one shared object that owns the settings, the live state and
//! every service (pipeline, server, OBS bridge, sessions, sound, hotkeys,
//! Host A, tray), applies setting changes to them, and fans display
//! messages out to the hosts.
//!
//! Threads:
//! * main: Tauri's event loop. Window work runs here; tray clicks, hotkeys
//!   and window events only queue work or flip atomics, never block.
//! * keystage-pipeline: input to display events (see `pipeline.rs`).
//! * keystage-actions: carries out tray and hotkey actions.
//! * keystage-house: emits `keystage://status` and updates the tray when
//!   something changed (and once a second for the recording timecode),
//!   watches the monitors, ends a hotkey capture that ran out.
//! * the server, OBS client and sound threads of their modules.
//!
//! Locks are short and never held while waiting for the main thread.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use keystage_core::ipc::{HookStatus, Monitor};
use keystage_core::protocol::{AppState, Legends, ServerMsg, SolidworksMode};
use keystage_core::{builtin, DisplayEvent, Profile, Settings};
use keystage_obs::ObsEvent;
use parking_lot::{Mutex, RwLock};
use tauri::{Emitter, EventTarget, Manager, Wry};

use super::actions::Action;
use super::hotkeys::HotkeyManager;
use super::obs_bridge::{self, BridgeChange, ObsBridge, RecordTracker, SessionStep};
use super::overlay_window::{self, OverlayManager};
use super::pipeline::{self, Control, Core, PipelineHandle, Sink, Started};
use super::server::{ServerManager, TauriAssets};
use super::sessions::{Origin, Sessions};
use super::settings_store::{self, io_words, SettingsStore};
use super::settings_window;
use super::sound::SoundHandle;
use super::state::{self, Live, PipelineView, StateInputs, StatusPayload};
use super::tray::{Tray, TrayView};
use super::{icons, logging};
use crate::platform::{self, InputHandle, WatcherHandle};

pub type App = tauri::AppHandle<Wry>;

/// The event Host A and the settings preview listen to.
pub const OVERLAY_EVENT: &str = "keystage://overlay";
/// The event the settings window listens to for `Status`.
pub const STATUS_EVENT: &str = "keystage://status";
/// The event that tells the settings window the stored settings changed
/// (tray, hotkey, import, reset or its own save). The window reads them
/// again with `get_settings`; the payload leaves the password blob out.
pub const SETTINGS_EVENT: &str = "keystage://settings";
/// The argument the autostart entry passes: start in the tray.
pub const AUTOSTART_ARG: &str = "--autostart";
/// How long UI Automation may take to subscribe after start before its
/// absence is a notice (the watcher itself waits up to 3 s for it).
pub const UIA_GRACE: Duration = Duration::from_secs(5);
/// The notice while UI Automation password detection does not run. The
/// pipeline masks typing in every profile meanwhile (`pipeline::Core`).
pub const UIA_NOTICE: &str = "Password detection for browsers and modern apps is unavailable, so typing is masked in every profile. Restart IDEA Keystage to try again.";

/// The notice after the pipeline recovered from an internal error.
pub const PIPELINE_RECOVERED_NOTICE: &str = "IDEA Keystage recovered from an internal error and cleared the keys on screen. If keys or clicks stop showing, quit IDEA Keystage from the tray and start it again.";
/// The notice after the pipeline gave up (`pipeline::MAX_RECOVERIES`).
pub const PIPELINE_STOPPED_NOTICE: &str = "IDEA Keystage stopped showing keys and clicks after repeated internal errors. Quit it from the tray and start it again.";

/// The password detection notice: shown while the watcher runs without UI
/// Automation, once the start grace period is over.
pub fn detection_notice(
    watcher_running: bool,
    uia_active: bool,
    since_start: Duration,
) -> Option<&'static str> {
    (watcher_running && !uia_active && since_start >= UIA_GRACE).then_some(UIA_NOTICE)
}

/// Wall clock in Unix milliseconds.
pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The folders the app uses.
#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
    /// Documents\IDEA Keystage\Sessions.
    pub default_sessions: PathBuf,
}

impl Paths {
    pub fn resolve(app: &App) -> Result<Paths, String> {
        let p = app.path();
        let config_dir = p
            .app_config_dir()
            .map_err(|e| format!("no settings folder: {e}"))?;
        let log_dir = p.app_log_dir().unwrap_or_else(|_| config_dir.join("logs"));
        let default_sessions = p
            .document_dir()
            .map(|d| d.join("IDEA Keystage").join("Sessions"))
            .or_else(|_| p.app_data_dir().map(|d| d.join("Sessions")))
            .unwrap_or_else(|_| config_dir.join("Sessions"));
        Ok(Paths {
            config_dir,
            log_dir,
            default_sessions,
        })
    }

    /// The sessions folder for these settings.
    pub fn sessions(&self, settings: &Settings) -> PathBuf {
        match settings.session.folder.as_deref() {
            Some(f) if !f.trim().is_empty() => PathBuf::from(f),
            _ => self.default_sessions.clone(),
        }
    }
}

/// The `keystage://settings` payload: the settings without the stored
/// password blob (the window reads them again with `get_settings` anyway).
pub fn settings_event_payload(settings: &Settings) -> Settings {
    let mut out = settings.clone();
    out.obs.password_dpapi = None;
    out
}

/// Should the Tauri event go to this webview? Listeners registered with no
/// target (the `@tauri-apps/api` default) receive every emit anyway; this
/// filter matters for listeners that name their own window.
pub fn wants_overlay_event(t: &EventTarget, overlay: bool, preview: bool) -> bool {
    match t {
        EventTarget::WebviewWindow { label }
        | EventTarget::Webview { label }
        | EventTarget::Window { label }
        | EventTarget::AnyLabel { label } => {
            (overlay && label.starts_with(overlay_window::LABEL_PREFIX))
                || (preview && label == settings_window::LABEL)
        }
        _ => false,
    }
}

pub struct Shell {
    me: Weak<Shell>,
    pub app: App,
    pub paths: Paths,
    store: SettingsStore,
    settings: RwLock<Arc<Settings>>,
    settings_write: Mutex<()>,
    pub builtin: Arc<Profile>,
    live: Mutex<Live>,
    legends: Mutex<Option<Legends>>,
    pub server: ServerManager,
    pub obs: ObsBridge,
    obs_tracker: Mutex<RecordTracker>,
    pub sessions: Sessions,
    pub sound: Option<SoundHandle>,
    pub hotkeys: Arc<HotkeyManager>,
    pub overlay: OverlayManager,
    pipeline: OnceLock<PipelineHandle>,
    input: OnceLock<InputHandle>,
    watcher: OnceLock<WatcherHandle>,
    tray: OnceLock<Tray<Wry>>,
    actions: Sender<Action>,
    refresh: Sender<()>,
    preview_visible: AtomicBool,
    last_status: Mutex<Option<StatusPayload>>,
    last_state: Mutex<Option<AppState>>,
    notices: Mutex<BTreeMap<&'static str, String>>,
    monitors: Mutex<Vec<Monitor>>,
    tray_size: AtomicU32,
    quitting: AtomicBool,
    shut_down: AtomicBool,
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell").finish_non_exhaustive()
    }
}

/// The pipeline's output, wired to the shell.
struct ShellSink {
    shell: Arc<Shell>,
}

impl Sink for ShellSink {
    fn display(&mut self, ev: &DisplayEvent, json: Arc<str>) {
        self.shell.publish(json);
        match ev {
            DisplayEvent::Callout(c) => self.shell.sessions.lock().on_callout(c),
            DisplayEvent::Key {
                key, down: true, ..
            } => {
                if let Some(s) = &self.shell.sound {
                    s.key_down(key);
                }
            }
            _ => {}
        }
    }

    fn legends(&mut self, legends: Legends, json: Arc<str>) {
        *self.shell.legends.lock() = Some(legends);
        self.shell.publish(json);
    }

    fn view(&mut self, view: PipelineView) {
        self.shell.live.lock().view = view;
        self.shell.publish_state();
        self.shell.refresh_soon();
    }

    fn failed(&mut self, stopped: bool) {
        let text = if stopped {
            PIPELINE_STOPPED_NOTICE
        } else {
            PIPELINE_RECOVERED_NOTICE
        };
        self.shell.live.lock().stopped = stopped;
        self.shell.set_notice("pipeline", Some(text.to_string()));
    }

    fn foreground_changed(&mut self) {
        let weak = self.shell.me.clone();
        let _ = self.shell.app.run_on_main_thread(move || {
            if let Some(s) = weak.upgrade() {
                s.overlay.reassert_topmost(&s.app);
            }
        });
    }
}

/// Sets everything up. Runs on the main thread inside Tauri's `setup`.
pub fn setup(app: &mut tauri::App<Wry>, autostarted: bool) -> Result<(), String> {
    let handle = app.handle().clone();
    let paths = Paths::resolve(&handle)?;
    let log_error = logging::init(&paths.log_dir).err();
    tracing::info!(
        version = keystage_core::protocol::APP_VERSION,
        autostarted,
        "IDEA Keystage starting"
    );
    let store = SettingsStore::new(&paths.config_dir);
    let loaded = store.load(&keystage_session::time::session_id(unix_ms()));
    let settings = Arc::new(loaded.settings);
    let mut notices = BTreeMap::new();
    if let Some(n) = loaded.notice.clone() {
        notices.insert("settings", n);
    }
    if let Some(e) = log_error {
        notices.insert(
            "log",
            format!("The log file could not be opened ({e}). The app works without it."),
        );
    }
    let builtin = match builtin::solidworks() {
        Ok(p) => Arc::new(p),
        Err(_) => {
            tracing::error!("built-in SolidWorks profile could not be read");
            notices.insert(
                "profile",
                "The built-in SolidWorks shortcut list could not be read, so no SolidWorks captions are shown."
                    .into(),
            );
            Arc::new(Profile {
                id: keystage_core::profile::SOLIDWORKS_ID.into(),
                name: "SolidWorks".into(),
                ..Profile::default()
            })
        }
    };
    let hub = keystage_server::Hub::new();
    let assets = Arc::new(TauriAssets::new(handle.asset_resolver()));
    let server = ServerManager::new(hub, assets);
    let sound = match SoundHandle::spawn() {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::error!(error = %e, "sound thread did not start");
            None
        }
    };
    let (actions_tx, actions_rx) = crossbeam_channel::unbounded();
    let (refresh_tx, refresh_rx) = crossbeam_channel::bounded(1);
    let sessions = Sessions::new(paths.sessions(&settings), settings.session.autosave);
    let shell = Arc::new_cyclic(|me| Shell {
        me: me.clone(),
        app: handle.clone(),
        paths,
        store,
        settings: RwLock::new(settings.clone()),
        settings_write: Mutex::new(()),
        builtin,
        live: Mutex::new(Live::default()),
        legends: Mutex::new(None),
        server,
        obs: ObsBridge::new(),
        obs_tracker: Mutex::new(RecordTracker::default()),
        sessions,
        sound,
        hotkeys: Arc::new(HotkeyManager::new()),
        overlay: OverlayManager::new(),
        pipeline: OnceLock::new(),
        input: OnceLock::new(),
        watcher: OnceLock::new(),
        tray: OnceLock::new(),
        actions: actions_tx,
        refresh: refresh_tx,
        preview_visible: AtomicBool::new(false),
        last_status: Mutex::new(None),
        last_state: Mutex::new(None),
        notices: Mutex::new(notices),
        monitors: Mutex::new(Vec::new()),
        tray_size: AtomicU32::new(16),
        quitting: AtomicBool::new(false),
        shut_down: AtomicBool::new(false),
    });
    app.manage(shell.clone());
    shell.start(actions_rx, refresh_rx, autostarted, loaded.notice)
}

impl Shell {
    fn arc(&self) -> Option<Arc<Shell>> {
        self.me.upgrade()
    }

    fn start(
        self: &Arc<Self>,
        actions_rx: Receiver<Action>,
        refresh_rx: Receiver<()>,
        autostarted: bool,
        reset_notice: Option<String>,
    ) -> Result<(), String> {
        let settings = self.settings();

        // The hello every new WebSocket client gets.
        let weak = self.me.clone();
        self.server
            .hub()
            .set_snapshot(move || match weak.upgrade() {
                Some(s) => s.hello().to_json(),
                None => ServerMsg::hello(Default::default(), Default::default(), None).to_json(),
            });
        self.server.ensure(settings.server.port);

        // Input, watcher and the pipeline.
        let core = Core::new(
            settings.clone(),
            self.builtin.clone(),
            platform::legends_snapshot(),
        );
        let weak = self.me.clone();
        let handle = pipeline::spawn(
            core,
            ShellSink {
                shell: self.clone(),
            },
            move |started: Started| {
                let Some(s) = weak.upgrade() else {
                    return;
                };
                match started.input {
                    Ok(h) => s.adopt_input(h),
                    Err(e) => s.set_notice(
                        "input",
                        Some(format!(
                            "Keyboard and mouse capture did not start ({e}). Restart IDEA Keystage."
                        )),
                    ),
                }
                match started.watcher {
                    Ok(h) => {
                        let _ = s.watcher.set(h);
                    }
                    Err(e) => s.set_notice(
                        "watcher",
                        Some(format!(
                            "Password field and app detection did not start ({e}), so nothing is shown. Restart IDEA Keystage."
                        )),
                    ),
                }
                s.refresh_soon();
            },
        )
        .map_err(|e| format!("the pipeline thread did not start: {e}"))?;
        let _ = self.pipeline.set(handle);

        if let Some(s) = &self.sound {
            s.configure(&settings.sound);
        }
        let change = self.obs.apply(&settings.obs);
        self.attach_obs(change);
        self.hotkeys.apply(&self.app, &settings.hotkeys);

        // Monitors, Host A, the tray icon size.
        self.read_monitors();
        self.monitors_changed_inline();

        // The tray.
        let view = self.tray_view();
        let tray = Tray::build(&self.app, &view, |app, action| {
            if let Some(s) = app.try_state::<Arc<Shell>>() {
                s.dispatch(action);
            }
        })
        .map_err(|e| format!("the tray icon could not be created: {e}"))?;
        let _ = self.tray.set(tray);

        // Client counts from the server.
        let weak = self.me.clone();
        let mut counts = self.server.hub().watch_counts();
        tauri::async_runtime::spawn(async move {
            while counts.changed().await.is_ok() {
                let Some(s) = weak.upgrade() else { break };
                s.publish_state();
                s.refresh_soon();
            }
        });

        // Worker threads.
        let me = self.clone();
        thread::Builder::new()
            .name("keystage-actions".into())
            .spawn(move || {
                while let Ok(a) = actions_rx.recv() {
                    if me.quitting.load(Ordering::Acquire) {
                        break;
                    }
                    me.perform(a);
                }
            })
            .map_err(|e| format!("the action thread did not start: {e}"))?;
        let me = self.clone();
        thread::Builder::new()
            .name("keystage-house".into())
            .spawn(move || me.housekeeping(refresh_rx))
            .map_err(|e| format!("the housekeeping thread did not start: {e}"))?;

        self.sync_autostart(settings.startup.start_with_windows);

        if settings_window::open_at_start(
            settings.first_run_done,
            settings.startup.show_settings,
            autostarted,
        ) {
            self.open_settings();
        }
        if let Some(text) = reset_notice {
            use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
            self.app
                .dialog()
                .message(text)
                .title("IDEA Keystage")
                .kind(MessageDialogKind::Warning)
                .show(|_| {});
        }
        self.publish_state();
        self.refresh_soon();
        tracing::info!("IDEA Keystage started");
        Ok(())
    }

    /* --- Settings ---------------------------------------------------------- */

    pub fn settings(&self) -> Arc<Settings> {
        self.settings.read().clone()
    }

    /// Changes the settings with `f`, validates, saves and applies them.
    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<Arc<Settings>, String> {
        self.replace_settings(|old| {
            let mut next = old.clone();
            f(&mut next);
            Ok(next)
        })
    }

    /// Replaces the settings with what `f` builds from the current ones.
    pub fn replace_settings(
        &self,
        f: impl FnOnce(&Settings) -> Result<Settings, String>,
    ) -> Result<Arc<Settings>, String> {
        let _w = self.settings_write.lock();
        let old = self.settings();
        let mut next = f(&old)?;
        let issues = next.validate();
        if !issues.is_empty() {
            tracing::info!(fields = ?settings_store::issue_fields(&issues), "settings adjusted");
        }
        if *old == next {
            return Ok(old);
        }
        self.store
            .save(&next)
            .map_err(|e| format!("Could not save the settings: {}.", io_words(&e)))?;
        let next = Arc::new(next);
        *self.settings.write() = next.clone();
        self.apply_changes(&old, &next);
        self.announce_settings(&next);
        Ok(next)
    }

    /// Tells the settings window that the stored settings changed, so a
    /// change made from the tray, a hotkey, an import or a reset shows at
    /// once. Only that window is addressed.
    fn announce_settings(&self, next: &Settings) {
        let target = EventTarget::webview_window(settings_window::LABEL);
        if let Err(e) = self
            .app
            .emit_to(target, SETTINGS_EVENT, settings_event_payload(next))
        {
            tracing::warn!(error = %e, "settings change not announced");
        }
    }

    fn apply_changes(&self, old: &Settings, new: &Arc<Settings>) {
        if let Some(p) = self.pipeline.get() {
            p.send(Control::Settings(new.clone()));
        }
        if old.overlay != new.overlay {
            let msg = ServerMsg::Config {
                config: new.overlay.clone(),
            };
            self.publish(Arc::from(msg.to_json()));
        }
        if old.host != new.host {
            self.monitors_changed();
        }
        if old.server.port != new.server.port {
            self.server.ensure(new.server.port);
        }
        if old.hotkeys != new.hotkeys && !self.hotkeys.capturing() {
            self.hotkeys.apply(&self.app, &new.hotkeys);
        }
        if old.sound != new.sound {
            if let Some(s) = &self.sound {
                s.configure(&new.sound);
            }
        }
        if old.obs != new.obs {
            if !new.obs.enabled {
                self.obs.set_chapter_notice(None);
            }
            let change = self.obs.apply(&new.obs);
            self.attach_obs(change);
        }
        if old.session != new.session {
            self.sessions
                .lock()
                .set_folder(self.paths.sessions(new), new.session.autosave);
        }
        if old.startup.start_with_windows != new.startup.start_with_windows {
            self.sync_autostart(new.startup.start_with_windows);
        }
        self.publish_state();
        self.refresh_soon();
    }

    fn sync_autostart(&self, enabled: bool) {
        use tauri_plugin_autostart::ManagerExt;
        if self
            .app
            .try_state::<tauri_plugin_autostart::AutoLaunchManager>()
            .is_none()
        {
            return;
        }
        let al = self.app.autolaunch();
        let current = al.is_enabled().unwrap_or(false);
        let result = match (enabled, current) {
            (true, false) => al.enable(),
            (false, true) => al.disable(),
            _ => Ok(()),
        };
        match result {
            Ok(()) => self.set_notice("autostart", None),
            Err(e) => {
                tracing::warn!(error = %e, "start with Windows not changed");
                self.set_notice(
                    "autostart",
                    Some(if enabled {
                        "Starting with Windows could not be turned on.".into()
                    } else {
                        "Starting with Windows could not be turned off.".into()
                    }),
                );
            }
        }
    }

    /* --- Fan-out ------------------------------------------------------------ */

    /// Sends one serialized protocol message to Host A, the settings preview
    /// (while it can be seen) and every WebSocket client.
    pub fn publish(&self, json: Arc<str>) {
        let overlay = self.overlay.is_open();
        let preview = self.preview_visible.load(Ordering::Acquire);
        if overlay || preview {
            let _ = self
                .app
                .emit_str_filter(OVERLAY_EVENT, json.to_string(), |t| {
                    wants_overlay_event(t, overlay, preview)
                });
        }
        self.server.hub().publish(json);
    }

    /// The wire `AppState` now.
    pub fn app_state(&self) -> AppState {
        let settings = self.settings();
        let live = self.live.lock().clone();
        let counts = self.server.hub().counts();
        let recording = self.obs.recording();
        let session = self.sessions.lock().state();
        state::app_state(&StateInputs {
            settings: &settings,
            live: &live,
            obs_hosts: counts.obs,
            sources: counts.sources(),
            recording,
            session,
        })
    }

    /// Publishes a `state` message when the state changed.
    pub fn publish_state(&self) {
        let mut last = self.last_state.lock();
        let st = self.app_state();
        if last.as_ref() != Some(&st) {
            let json = ServerMsg::State { state: st.clone() }.to_json();
            *last = Some(st);
            self.publish(Arc::from(json));
        }
    }

    /// The `hello` for Host A, the preview and new WebSocket clients.
    pub fn hello(&self) -> ServerMsg {
        let settings = self.settings();
        ServerMsg::hello(
            settings.overlay.clone(),
            self.app_state(),
            self.legends.lock().clone(),
        )
    }

    /* --- Status ------------------------------------------------------------- */

    fn set_notice(&self, key: &'static str, text: Option<String>) {
        let mut n = self.notices.lock();
        match text {
            Some(t) => {
                n.insert(key, t);
            }
            None => {
                n.remove(key);
            }
        }
        drop(n);
        self.refresh_soon();
    }

    pub fn clear_settings_notice(&self) {
        self.set_notice("settings", None);
    }

    /// Raises or clears the notice that UI Automation password detection
    /// does not run (only classic Edit password boxes are detected then).
    fn check_password_detection(&self, since_start: Duration) {
        let Some(w) = self.watcher.get() else {
            return;
        };
        let want = detection_notice(w.is_running(), w.uia_active(), since_start);
        let shown = self.notices.lock().get("uia").map(String::as_str) == want;
        if shown {
            return;
        }
        if want.is_some() {
            tracing::warn!("UI Automation password detection is not running; typing is masked");
        }
        self.set_notice("uia", want.map(str::to_string));
    }

    fn notices(&self) -> Vec<String> {
        let mut v: Vec<String> = self.notices.lock().values().cloned().collect();
        v.extend(self.hotkeys.notices());
        v.extend(self.obs.notices());
        if let Some(e) = self.sound.as_ref().and_then(|s| s.error()) {
            v.push(e);
        }
        v
    }

    /// `get_status`.
    pub fn status(&self) -> StatusPayload {
        let settings = self.settings();
        let live = self.live.lock().clone();
        let hook = match self.input.get() {
            Some(h) => HookStatus {
                installed: h.is_installed(),
                dropped: h.dropped(),
            },
            None => HookStatus::default(),
        };
        let session = self.sessions.lock().status();
        StatusPayload {
            status: state::status(
                &settings,
                &live,
                self.server.status(),
                self.obs.status(settings.obs.enabled),
                session,
                hook,
            ),
            notices: self.notices(),
        }
    }

    /// Asks the housekeeping thread to emit the status and update the tray.
    pub fn refresh_soon(&self) {
        let _ = self.refresh.try_send(());
    }

    fn tray_view(&self) -> TrayView {
        let settings = self.settings();
        let live = self.live.lock().clone();
        let recording = self.obs.recording();
        let session_active = self.sessions.lock().is_active();
        let obs_recording = recording.as_ref().is_some_and(|r| r.active);
        TrayView {
            status_line: state::tray_status_line(&live, recording.as_ref()),
            tooltip: state::tray_tooltip(&live, recording.as_ref()),
            glyph: state::tray_glyph(&live, recording.as_ref()),
            size: self.tray_size.load(Ordering::Acquire),
            paused: live.paused,
            keyboard_visible: live.keyboard_visible,
            mode: settings.profiles.solidworks_mode,
            native_overlay: settings.host.native_overlay,
            session_active,
            marker_enabled: session_active || (settings.obs.enabled && obs_recording),
        }
    }

    /// Emits the status when it changed and updates the tray. Waits for the
    /// main thread (tray), so only the housekeeping thread calls it.
    fn refresh_now(&self) {
        let payload = self.status();
        let changed = {
            let mut last = self.last_status.lock();
            if last.as_ref() != Some(&payload) {
                *last = Some(payload.clone());
                true
            } else {
                false
            }
        };
        if changed {
            let _ = self.app.emit(STATUS_EVENT, &payload);
        }
        if let Some(t) = self.tray.get() {
            t.update(&self.tray_view());
        }
    }

    fn housekeeping(&self, refresh_rx: Receiver<()>) {
        let started = Instant::now();
        let mut last_monitor_poll = Instant::now();
        while let Ok(()) | Err(RecvTimeoutError::Timeout) =
            refresh_rx.recv_timeout(Duration::from_secs(1))
        {
            if self.quitting.load(Ordering::Acquire) {
                break;
            }
            self.check_password_detection(started.elapsed());
            self.publish_state();
            self.refresh_now();
            if last_monitor_poll.elapsed() >= Duration::from_secs(2) {
                last_monitor_poll = Instant::now();
                if self.read_monitors() {
                    self.monitors_changed();
                }
            }
            if self.hotkeys.capture_expired(Instant::now()) {
                let settings = self.settings();
                self.hotkeys.apply(&self.app, &settings.hotkeys);
                self.refresh_soon();
            }
        }
    }

    /* --- Monitors and Host A ------------------------------------------------ */

    /// Reads the monitors; true when the list changed.
    fn read_monitors(&self) -> bool {
        match platform::monitors() {
            Ok(ms) => {
                let mut cur = self.monitors.lock();
                if *cur != ms {
                    *cur = ms;
                    true
                } else {
                    false
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "monitors could not be read");
                false
            }
        }
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        self.monitors.lock().clone()
    }

    fn monitor_updates(&self) -> (Option<keystage_core::MonitorRect>, u32) {
        let settings = self.settings();
        let ms = self.monitors();
        let rect = overlay_window::monitor_rect(&ms, &settings);
        let scale = ms
            .iter()
            .find(|m| m.primary)
            .map(|m| m.scale)
            .unwrap_or(1.0);
        (rect, icons::size_for_scale(scale))
    }

    /// From setup, on the main thread.
    fn monitors_changed_inline(&self) {
        let (rect, size) = self.monitor_updates();
        if let Some(p) = self.pipeline.get() {
            p.send(Control::Monitor(rect));
        }
        self.tray_size.store(size, Ordering::Release);
        let settings = self.settings();
        let desired = overlay_window::placement(&self.monitors(), &settings);
        self.overlay
            .reconcile(&self.app, desired, settings.host.exclude_from_capture);
    }

    /// From any thread but the main one: the window work is queued there.
    fn monitors_changed(&self) {
        let (rect, size) = self.monitor_updates();
        if let Some(p) = self.pipeline.get() {
            p.send(Control::Monitor(rect));
        }
        self.tray_size.store(size, Ordering::Release);
        let settings = self.settings();
        let desired = overlay_window::placement(&self.monitors(), &settings);
        let exclude = settings.host.exclude_from_capture;
        let weak = self.me.clone();
        let _ = self.app.run_on_main_thread(move || {
            if let Some(s) = weak.upgrade() {
                s.overlay.reconcile(&s.app, desired, exclude);
                s.publish_state();
            }
        });
        self.refresh_soon();
    }

    /* --- Live switches ------------------------------------------------------ */

    /// Keeps the input handle the pipeline thread started. A pause asked
    /// for before it arrived (a hotkey or the window at a busy logon, while
    /// the watcher was still starting) removes the hooks now (ARCHITECTURE.md
    /// section 8, rule 2). The live lock orders this against `set_paused`,
    /// so the hooks always end up as the last pause or resume left them.
    fn adopt_input(&self, h: InputHandle) {
        let live = self.live.lock();
        if self.input.set(h).is_err() || !live.paused {
            return;
        }
        if let Some(h) = self.input.get() {
            if let Err(e) = h.pause() {
                tracing::warn!(error = %e, "hooks were not removed for the pause");
            }
        }
    }

    pub fn set_paused(&self, paused: bool) {
        {
            let mut l = self.live.lock();
            if l.paused == paused {
                return;
            }
            l.paused = paused;
            // Under the lock (pause and resume only post to the hook
            // thread): see `adopt_input`.
            if let Some(h) = self.input.get() {
                let r = if paused { h.pause() } else { h.resume() };
                if let Err(e) = r {
                    tracing::warn!(error = %e, paused, "hooks did not change");
                }
            }
        }
        if let Some(p) = self.pipeline.get() {
            p.send(Control::Paused(paused));
        }
        tracing::info!(paused, "pause changed");
        self.publish_state();
        self.refresh_soon();
    }

    pub fn set_keyboard_visible(&self, visible: bool) {
        {
            let mut l = self.live.lock();
            if l.keyboard_visible == visible {
                return;
            }
            l.keyboard_visible = visible;
        }
        self.publish_state();
        self.refresh_soon();
    }

    pub fn set_mode(&self, mode: SolidworksMode) -> Result<(), String> {
        self.update_settings(|s| s.profiles.solidworks_mode = mode)
            .map(|_| ())
    }

    pub fn set_native_overlay(&self, on: bool) -> Result<(), String> {
        self.update_settings(|s| s.host.native_overlay = on)
            .map(|_| ())
    }

    pub fn preview_visible(&self, visible: bool) {
        self.preview_visible.store(visible, Ordering::Release);
    }

    /* --- Sessions ----------------------------------------------------------- */

    pub fn session_start(&self, origin: Origin) {
        if self.sessions.start(unix_ms(), platform::now_ms(), origin) {
            tracing::info!(by_obs = origin == Origin::Obs, "session log started");
        }
        self.publish_state();
        self.refresh_soon();
    }

    pub fn session_stop(&self) -> Result<(), String> {
        let was = self.sessions.lock().is_active();
        let r = self.sessions.stop(unix_ms(), platform::now_ms());
        match &r {
            Ok(_) => {
                if was {
                    tracing::info!("session log stopped");
                }
                self.set_notice("session", None);
            }
            Err(e) => self.set_notice("session", Some(e.clone())),
        }
        self.publish_state();
        self.refresh_soon();
        r.map(|_| ())
    }

    /// A chapter marker: always in the running Session Log, and in OBS too
    /// when OBS is connected and records (best effort).
    pub fn add_marker(&self, name: Option<&str>) -> Result<(), String> {
        let settings = self.settings();
        let logged = self.sessions.lock().add_marker(platform::now_ms(), name);
        let obs_recording =
            settings.obs.enabled && self.obs.recording().is_some_and(|r| r.active && !r.paused);
        if settings.obs.enabled {
            self.obs.add_chapter(name);
        }
        self.publish_state();
        self.refresh_soon();
        match logged {
            Ok(_) => Ok(()),
            Err(_) if obs_recording => Ok(()),
            Err(e) => Err(e),
        }
    }

    /* --- OBS ---------------------------------------------------------------- */

    fn attach_obs(&self, change: BridgeChange) {
        let BridgeChange::Started {
            generation,
            mut events,
            mut status,
        } = change
        else {
            if !self.settings().obs.enabled {
                self.obs_tracker.lock().reset();
            }
            return;
        };
        self.obs_tracker.lock().reset();
        let weak = self.me.clone();
        tauri::async_runtime::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                tokio::select! {
                    ev = events.recv() => match ev {
                        Ok(ev) => {
                            let Some(s) = weak.upgrade() else { break };
                            if !s.obs.is_current(generation) { break; }
                            s.on_obs_event(ev);
                        }
                        Err(RecvError::Lagged(_)) => continue,
                        Err(RecvError::Closed) => break,
                    },
                    changed = status.changed() => {
                        if changed.is_err() { break; }
                        let Some(s) = weak.upgrade() else { break };
                        if !s.obs.is_current(generation) { break; }
                        s.publish_state();
                        s.refresh_soon();
                    }
                }
            }
        });
    }

    fn on_obs_event(&self, ev: ObsEvent) {
        let auto = self.settings().obs.auto_session;
        match ev {
            ObsEvent::Record { state, at } => {
                let t = obs_bridge::instant_to_ms(at, Instant::now(), platform::now_ms());
                let steps = self.obs_tracker.lock().on_state(state, auto);
                for step in steps {
                    match step {
                        SessionStep::StartSession => {
                            if self.sessions.start(unix_ms(), t, Origin::Obs) {
                                tracing::info!(by_obs = true, "session log started");
                            }
                        }
                        SessionStep::Record(e) => self.sessions.lock().record_event(t, e),
                        SessionStep::StopSession => {
                            let by_obs = self.sessions.lock().origin() == Some(Origin::Obs);
                            if by_obs {
                                let _ = self.session_stop();
                            }
                        }
                    }
                }
                if state == keystage_obs::RecordState::Recording {
                    self.obs.set_chapter_notice(None);
                }
            }
            ObsEvent::Sample(s) => {
                let t = obs_bridge::instant_to_ms(s.at, Instant::now(), platform::now_ms());
                self.sessions
                    .lock()
                    .clock_sample(t, s.duration_ms, s.paused);
            }
            ObsEvent::Chapter(o) => {
                if let Some(n) = obs_bridge::chapter_notice(&o) {
                    self.obs.set_chapter_notice(Some(n));
                }
            }
            ObsEvent::Disconnected | ObsEvent::Exiting => self.obs_tracker.lock().reset(),
            ObsEvent::Connected { .. } => {}
        }
        self.publish_state();
        self.refresh_soon();
    }

    /* --- Windows, actions, lifecycle ---------------------------------------- */

    /// Opens or focuses the settings window (queued on the main thread).
    pub fn open_settings(&self) {
        let weak = self.me.clone();
        let _ = self.app.run_on_main_thread(move || {
            let Some(s) = weak.upgrade() else { return };
            let w2 = weak.clone();
            let r = settings_window::open(&s.app, move |visible| {
                if let Some(s) = w2.upgrade() {
                    s.preview_visible(visible);
                }
            });
            if let Err(e) = r {
                tracing::error!(error = %e, "settings window did not open");
            }
        });
    }

    /// Queues an action for the action thread. Safe on the main thread.
    pub fn dispatch(&self, a: Action) {
        let _ = self.actions.send(a);
    }

    fn perform(&self, a: Action) {
        let result = match a {
            Action::TogglePause => {
                let p = self.live.lock().paused;
                self.set_paused(!p);
                Ok(())
            }
            Action::ToggleKeyboard => {
                let v = self.live.lock().keyboard_visible;
                self.set_keyboard_visible(!v);
                Ok(())
            }
            Action::SetMode(m) => self.set_mode(m),
            Action::CycleMode => {
                let m = self.settings().profiles.solidworks_mode;
                self.set_mode(state::next_mode(m))
            }
            Action::ToggleNativeOverlay => {
                let on = self.settings().host.native_overlay;
                self.set_native_overlay(!on)
            }
            Action::ToggleSession => {
                if self.sessions.lock().is_active() {
                    self.session_stop()
                } else {
                    self.session_start(Origin::User);
                    Ok(())
                }
            }
            Action::AddMarker => self.add_marker(None),
            Action::OpenSettings => {
                self.open_settings();
                Ok(())
            }
            Action::Quit => {
                self.quit();
                Ok(())
            }
        };
        if result.is_err() {
            tracing::info!(action = ?a, "action did not complete");
        }
        // A click on a check item toggles it by itself; show the truth.
        if let Some(t) = self.tray.get() {
            t.invalidate();
        }
        self.refresh_soon();
    }

    pub fn quit(&self) {
        self.quitting.store(true, Ordering::Release);
        self.app.exit(0);
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::Acquire)
    }

    /// Stops everything; saves a running Session Log. Idempotent.
    pub fn shutdown(&self) {
        if self.shut_down.swap(true, Ordering::AcqRel) {
            return;
        }
        self.quitting.store(true, Ordering::Release);
        tracing::info!("IDEA Keystage stopping");
        if self.sessions.stop(unix_ms(), platform::now_ms()).is_err() {
            tracing::warn!("session log not saved at exit");
        }
        if let Some(p) = self.pipeline.get() {
            p.shutdown();
        }
        if let Some(h) = self.input.get() {
            h.shutdown();
        }
        if let Some(w) = self.watcher.get() {
            w.shutdown();
        }
        self.obs.shutdown();
        self.server.stop();
        if let Some(s) = &self.sound {
            s.shutdown();
        }
        let _ = self.refresh.try_send(());
        tracing::info!("IDEA Keystage stopped");
    }

    /// For commands that need the shell in a `'static` closure.
    pub fn handle(&self) -> Option<Arc<Shell>> {
        self.arc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_events_go_to_host_a_and_a_visible_preview() {
        let host_a = EventTarget::WebviewWindow {
            label: "overlay-3".into(),
        };
        let settings = EventTarget::AnyLabel {
            label: "settings".into(),
        };
        assert!(wants_overlay_event(&host_a, true, false));
        assert!(!wants_overlay_event(&host_a, false, true));
        assert!(wants_overlay_event(&settings, false, true));
        assert!(!wants_overlay_event(&settings, true, false));
        assert!(!wants_overlay_event(&EventTarget::App, true, true));
        assert!(!wants_overlay_event(
            &EventTarget::WebviewWindow {
                label: "other".into()
            },
            true,
            true
        ));
    }

    #[test]
    fn the_settings_event_matches_the_window_and_leaves_the_password_out() {
        const IPC_TS: &str = include_str!("../../../src/settings/ipc.ts");
        for (name, value) in [
            ("SETTINGS_EVENT", SETTINGS_EVENT),
            ("STATUS_EVENT", STATUS_EVENT),
        ] {
            assert!(
                IPC_TS.contains(&format!("export const {name} = '{value}';")),
                "{name} differs from src/settings/ipc.ts"
            );
        }
        let mut s = Settings::default();
        s.obs.password_dpapi = Some("QUJD".into());
        s.obs.port = 4460;
        let out = settings_event_payload(&s);
        assert_eq!(out.obs.password_dpapi, None);
        assert_eq!(out.obs.port, 4460);
        let json = serde_json::to_string(&out).expect("payload");
        assert!(!json.contains("QUJD"));
    }

    #[test]
    fn the_clock_is_plausible() {
        assert!(unix_ms() > 1_700_000_000_000);
    }

    #[test]
    fn the_autostart_argument_is_a_flag() {
        assert!(AUTOSTART_ARG.starts_with("--"));
    }

    #[test]
    fn missing_password_detection_is_a_notice_after_the_grace_period() {
        let later = UIA_GRACE + Duration::from_secs(1);
        assert_eq!(detection_notice(true, false, later), Some(UIA_NOTICE));
        assert_eq!(detection_notice(true, true, later), None);
        assert_eq!(
            detection_notice(true, false, Duration::from_secs(1)),
            None,
            "UI Automation may still be subscribing"
        );
        assert_eq!(
            detection_notice(false, false, later),
            None,
            "a watcher that did not start has its own notice"
        );
        assert!(!UIA_NOTICE.contains('\u{2014}') && !UIA_NOTICE.contains('\u{2013}'));
    }
}
