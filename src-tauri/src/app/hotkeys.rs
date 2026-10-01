//! The five global hotkeys (ARCHITECTURE.md section 7), registered through
//! tauri-plugin-global-shortcut from `settings.hotkeys`. Registration
//! failures (another program or Windows holds the shortcut) are kept per
//! hotkey for `hotkeys_status` and turned into status notices. The same
//! accelerators reach the core as hidden combos through
//! `Settings::display_rules`, so the app's own hotkeys are never shown.
//!
//! The plugin calls the handler on the main thread while it holds its own
//! lock, so the handler only looks up the action and hands it to the action
//! worker; registering from inside it would deadlock. Registering and
//! unregistering always run on the main thread (see [`HotkeyManager::apply`]).

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use keystage_core::keys::parse_accelerator;
use keystage_core::settings::Hotkeys;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// While a hotkey field in settings waits for a shortcut, the hotkeys stay
/// unregistered at most this long.
pub const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a caller waits for the main thread to change the hotkeys.
pub const MAIN_THREAD_WAIT: Duration = Duration::from_secs(3);

/// One of the five hotkeys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyName {
    Pause,
    ToggleKeyboard,
    ToggleSolidworks,
    ChapterMarker,
    SessionToggle,
}

impl HotkeyName {
    pub const ALL: [HotkeyName; 5] = [
        HotkeyName::Pause,
        HotkeyName::ToggleKeyboard,
        HotkeyName::ToggleSolidworks,
        HotkeyName::ChapterMarker,
        HotkeyName::SessionToggle,
    ];

    /// The settings field name.
    pub fn field(self) -> &'static str {
        match self {
            HotkeyName::Pause => "pause",
            HotkeyName::ToggleKeyboard => "toggle_keyboard",
            HotkeyName::ToggleSolidworks => "toggle_solidworks",
            HotkeyName::ChapterMarker => "chapter_marker",
            HotkeyName::SessionToggle => "session_toggle",
        }
    }

    /// Sentence case words for notices.
    pub fn label(self) -> &'static str {
        match self {
            HotkeyName::Pause => "Pause",
            HotkeyName::ToggleKeyboard => "Keyboard visual",
            HotkeyName::ToggleSolidworks => "SolidWorks mode",
            HotkeyName::ChapterMarker => "Chapter marker",
            HotkeyName::SessionToggle => "Session log",
        }
    }

    pub fn accelerator(self, h: &Hotkeys) -> &str {
        match self {
            HotkeyName::Pause => &h.pause,
            HotkeyName::ToggleKeyboard => &h.toggle_keyboard,
            HotkeyName::ToggleSolidworks => &h.toggle_solidworks,
            HotkeyName::ChapterMarker => &h.chapter_marker,
            HotkeyName::SessionToggle => &h.session_toggle,
        }
    }
}

/// `hotkeys_status` entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HotkeyState {
    pub name: HotkeyName,
    pub accelerator: String,
    pub registered: bool,
    /// Why registration failed, in plain words; `None` when registered.
    pub error: Option<String>,
}

/// One hotkey ready to register, or why it cannot be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    pub name: HotkeyName,
    pub accelerator: String,
    pub shortcut: Result<Shortcut, String>,
}

pub const TAKEN: &str =
    "Another program or Windows already uses this shortcut. Choose another one.";
pub const UNREGISTRABLE: &str = "This shortcut cannot be a global hotkey. Choose another one.";

/// Turns the settings into shortcuts the plugin understands (Win is
/// `Super` there). A shortcut used twice is registered once, for the first
/// hotkey in contract order.
pub fn plan(h: &Hotkeys) -> Vec<Planned> {
    let mut seen: HashMap<u32, HotkeyName> = HashMap::new();
    HotkeyName::ALL
        .iter()
        .map(|&name| {
            let accelerator = name.accelerator(h).to_string();
            let shortcut = parse_accelerator(&accelerator)
                .ok()
                .and_then(|a| a.to_global_shortcut())
                .and_then(|s| Shortcut::from_str(&s).ok())
                .ok_or_else(|| UNREGISTRABLE.to_string())
                .and_then(|sc| match seen.get(&sc.id()) {
                    Some(other) => Err(format!(
                        "The {} hotkey already uses this shortcut.",
                        other.label()
                    )),
                    None => {
                        seen.insert(sc.id(), name);
                        Ok(sc)
                    }
                });
            Planned {
                name,
                accelerator,
                shortcut,
            }
        })
        .collect()
}

/// Notices for the status: one per hotkey that is not registered.
pub fn notices(states: &[HotkeyState]) -> Vec<String> {
    states
        .iter()
        .filter(|s| !s.registered)
        .map(|s| {
            format!(
                "The {} hotkey ({}) is not active: {}",
                s.label_lower(),
                s.accelerator,
                s.error.as_deref().unwrap_or(TAKEN)
            )
        })
        .collect()
}

impl HotkeyState {
    fn label_lower(&self) -> String {
        let l = self.name.label();
        if l.starts_with("SolidWorks") {
            l.to_string()
        } else {
            l.to_lowercase()
        }
    }
}

/// Registers the hotkeys and maps plugin events back to them.
#[derive(Debug, Default)]
pub struct HotkeyManager {
    states: Mutex<Vec<HotkeyState>>,
    ids: Mutex<HashMap<u32, HotkeyName>>,
    capture_until: Mutex<Option<Instant>>,
    apply_lock: Mutex<()>,
}

impl HotkeyManager {
    pub fn new() -> HotkeyManager {
        HotkeyManager::default()
    }

    /// Unregisters the app's hotkeys and registers them from `h`, on the
    /// main thread, and waits (at most a few seconds) until that is done.
    ///
    /// The plugin's `unregister_all` holds its shortcut map locked while it
    /// waits for the main thread, and its event handler takes that lock on
    /// the main thread, so calling it from another thread could deadlock
    /// when a hotkey is pressed at that moment. On the main thread both run
    /// one after the other. Never call this from the plugin's handler.
    pub fn apply<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>, h: &Hotkeys) {
        let me = self.clone();
        let handle = app.clone();
        let h = h.clone();
        self.on_main(app, move || me.apply_here(&handle, &h));
    }

    /// `hotkeys_capture`: while `active`, the hotkeys are unregistered so a
    /// shortcut typed into a settings field reaches the window. They come
    /// back on `false` or after [`CAPTURE_TIMEOUT`].
    pub fn capture<R: Runtime>(self: &Arc<Self>, app: &AppHandle<R>, active: bool, h: &Hotkeys) {
        if !active {
            self.apply(app, h);
            return;
        }
        let me = self.clone();
        let handle = app.clone();
        self.on_main(app, move || {
            if let Err(e) = handle.global_shortcut().unregister_all() {
                tracing::warn!(error = %e, "could not pause hotkeys");
            }
            me.ids.lock().clear();
            *me.capture_until.lock() = Some(Instant::now() + CAPTURE_TIMEOUT);
        });
    }

    /// Runs `f` on the main thread (inline when already there) and waits.
    fn on_main<R: Runtime, F: FnOnce() + Send + 'static>(&self, app: &AppHandle<R>, f: F) {
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
        let queued = app.run_on_main_thread(move || {
            f();
            let _ = tx.send(());
        });
        if queued.is_err() {
            tracing::warn!("hotkeys not changed: the app is closing");
            return;
        }
        if rx.recv_timeout(MAIN_THREAD_WAIT).is_err() {
            tracing::warn!("hotkey change still pending on the main thread");
        }
    }

    fn apply_here<R: Runtime>(&self, app: &AppHandle<R>, h: &Hotkeys) {
        let _guard = self.apply_lock.lock();
        *self.capture_until.lock() = None;
        let gs = app.global_shortcut();
        if let Err(e) = gs.unregister_all() {
            tracing::warn!(error = %e, "could not unregister hotkeys");
        }
        let mut states = Vec::with_capacity(5);
        let mut ids = HashMap::new();
        for p in plan(h) {
            let (registered, error) = match p.shortcut {
                Ok(sc) => match gs.register(sc) {
                    Ok(()) => {
                        ids.insert(sc.id(), p.name);
                        (true, None)
                    }
                    Err(_) => (false, Some(TAKEN.to_string())),
                },
                Err(why) => (false, Some(why)),
            };
            states.push(HotkeyState {
                name: p.name,
                accelerator: p.accelerator,
                registered,
                error,
            });
        }
        let failed = states.iter().filter(|s| !s.registered).count();
        tracing::info!(registered = 5 - failed, failed, "hotkeys registered");
        *self.ids.lock() = ids;
        *self.states.lock() = states;
    }

    /// True when a capture ran out; the caller then re-registers.
    pub fn capture_expired(&self, now: Instant) -> bool {
        matches!(*self.capture_until.lock(), Some(t) if now >= t)
    }

    pub fn capturing(&self) -> bool {
        self.capture_until.lock().is_some()
    }

    /// Which hotkey a plugin event belongs to.
    pub fn lookup(&self, id: u32) -> Option<HotkeyName> {
        self.ids.lock().get(&id).copied()
    }

    pub fn states(&self) -> Vec<HotkeyState> {
        self.states.lock().clone()
    }

    pub fn notices(&self) -> Vec<String> {
        notices(&self.states.lock())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_plan_five_distinct_shortcuts() {
        let p = plan(&Hotkeys::default());
        assert_eq!(p.len(), 5);
        assert!(p.iter().all(|x| x.shortcut.is_ok()), "{p:?}");
        let ids: std::collections::HashSet<u32> = p
            .iter()
            .map(|x| x.shortcut.as_ref().unwrap().id())
            .collect();
        assert_eq!(ids.len(), 5);
        assert_eq!(p[0].name, HotkeyName::Pause);
        assert_eq!(p[0].accelerator, "Ctrl+Alt+Shift+P");
        let expect = Shortcut::from_str("Ctrl+Alt+Shift+P").expect("parses");
        assert_eq!(p[0].shortcut.as_ref().unwrap().id(), expect.id());
    }

    #[test]
    fn win_becomes_super_and_bad_ones_are_reported() {
        let mut h = Hotkeys::default();
        h.pause = "Win+Shift+F9".into();
        h.toggle_keyboard = "Ctrl+Menu".into();
        h.chapter_marker = h.session_toggle.clone();
        let p = plan(&h);
        let sc = p[0].shortcut.as_ref().expect("Win registers as Super");
        assert_eq!(
            sc.id(),
            Shortcut::from_str("Super+Shift+F9").expect("p").id()
        );
        assert_eq!(p[1].shortcut, Err(UNREGISTRABLE.to_string()));
        assert!(p[3].shortcut.is_ok(), "the first use wins");
        assert_eq!(
            p[4].shortcut,
            Err("The Chapter marker hotkey already uses this shortcut.".into())
        );
    }

    #[test]
    fn notices_name_the_hotkey_in_plain_words() {
        let states = vec![
            HotkeyState {
                name: HotkeyName::Pause,
                accelerator: "Ctrl+Alt+Shift+P".into(),
                registered: true,
                error: None,
            },
            HotkeyState {
                name: HotkeyName::ToggleSolidworks,
                accelerator: "Ctrl+Alt+Shift+S".into(),
                registered: false,
                error: Some(TAKEN.into()),
            },
            HotkeyState {
                name: HotkeyName::SessionToggle,
                accelerator: "Ctrl+Alt+Shift+L".into(),
                registered: false,
                error: None,
            },
        ];
        let n = notices(&states);
        assert_eq!(n.len(), 2);
        assert!(n[0].starts_with("The SolidWorks mode hotkey (Ctrl+Alt+Shift+S) is not active"));
        assert!(n[1].starts_with("The session log hotkey"));
        for s in &n {
            assert!(!s.contains('\u{2014}'));
        }
    }

    #[test]
    fn state_serializes_like_the_ipc_client_expects() {
        let s = HotkeyState {
            name: HotkeyName::ToggleKeyboard,
            accelerator: "Ctrl+Alt+Shift+K".into(),
            registered: true,
            error: None,
        };
        let v = serde_json::to_value(&s).expect("v");
        assert_eq!(v["name"], "toggle_keyboard");
        assert_eq!(v["error"], serde_json::Value::Null);
        for n in HotkeyName::ALL {
            assert_eq!(serde_json::to_value(n).expect("v"), n.field());
        }
    }

    #[test]
    fn capture_expiry() {
        let m = HotkeyManager::new();
        assert!(!m.capturing());
        assert!(!m.capture_expired(Instant::now()));
        *m.capture_until.lock() = Some(Instant::now());
        assert!(m.capturing());
        assert!(m.capture_expired(Instant::now() + Duration::from_millis(1)));
    }
}
