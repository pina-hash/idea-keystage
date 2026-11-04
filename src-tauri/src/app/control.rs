//! The IDEA Keystage connector's control API in the app (ARCHITECTURE.md
//! section 14): the token in `control.json`, and every tool of
//! `keystage_automation::tools::catalog()`.
//!
//! Privacy: the log names the tool and nothing it was given. Recording
//! files can be read only inside OBS's recording folder or the Session Log
//! folder. Window titles are never read.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Weak;

use base64::Engine;
use keystage_automation::motion::Rect;
use keystage_automation::paths;
use keystage_automation::script::{Button, Direction, Idle, Script, Target};
use keystage_automation::token::{self, ControlFile, CONTROL_FILE};
use keystage_automation::tools;
use keystage_core::protocol::{SolidworksMode, APP_VERSION};
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use super::automation::{self, Runner};
use super::sessions::{self, Origin};
use super::shell::Shell;
use crate::platform::{self, automation as os};

/// Reads (or makes) the token and writes `control.json` with the port.
pub fn write_control_file(dir: &Path, port: u16) -> Result<String, String> {
    let path = dir.join(CONTROL_FILE);
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<ControlFile>(&t).ok())
        .map(|f| f.token)
        .filter(|t| token::is_token(t));
    let tok = match existing {
        Some(t) => t,
        None => {
            let bytes = platform::random_bytes().ok_or("no random numbers")?;
            token::token_from_bytes(&bytes)
        }
    };
    let f = ControlFile {
        port,
        token: tok.clone(),
        version: APP_VERSION.to_string(),
    };
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{CONTROL_FILE}.tmp"));
    let text = serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(tok)
}

/// The connector, handed to the local server.
pub struct ControlService {
    shell: Weak<Shell>,
    token: Mutex<String>,
    running: AtomicBool,
    show_saved: Mutex<Option<[bool; 4]>>,
    record_dir: Mutex<Option<PathBuf>>,
}

fn args<T: DeserializeOwned>(v: &Value) -> Result<T, String> {
    let v = if v.is_null() { json!({}) } else { v.clone() };
    serde_json::from_value(v).map_err(|e| format!("The arguments do not fit this tool: {e}."))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct None0 {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartArgs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    capture: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerArgs {
    name: String,
    #[serde(default)]
    step: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VisibleArgs {
    visible: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeArgs {
    mode: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArgs {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkArgs {
    path: String,
    offset: u64,
    length: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MonitorArgs {
    #[serde(default)]
    monitor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveArgs {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    monitor: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClickArgs {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    monitor: Option<String>,
    #[serde(default)]
    button: Button,
    #[serde(default)]
    double: bool,
    #[serde(default)]
    wait_idle: Option<Idle>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DragArgs {
    path: Vec<[f64; 2]>,
    #[serde(default)]
    button: Button,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScrollArgs {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    monitor: Option<String>,
    clicks: u32,
    direction: Direction,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeysArgs {
    keys: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextArgs {
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdleArgs {
    rect: Rect,
    #[serde(default = "idle_default")]
    max_ms: u64,
}

fn idle_default() -> u64 {
    3_000
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// The obs-websocket "Connect to OBS" switch is off: what to do.
pub const OBS_OFF: &str = "The OBS link is off. In IDEA Keystage settings, open the OBS page and turn on Connect to OBS. In OBS, open Tools, WebSocket Server Settings, and turn on Enable WebSocket server (the port and password there go into IDEA Keystage).";

impl ControlService {
    pub fn new(shell: Weak<Shell>) -> ControlService {
        ControlService {
            shell,
            token: Mutex::new(String::new()),
            running: AtomicBool::new(false),
            show_saved: Mutex::new(None),
            record_dir: Mutex::new(None),
        }
    }

    pub fn set_token(&self, t: String) {
        *self.token.lock() = t;
    }

    /// One input run at a time.
    fn input<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        if self.running.swap(true, Ordering::AcqRel) {
            return Err("Another automated run is still going. Wait for it to finish.".into());
        }
        let r = f();
        self.running.store(false, Ordering::Release);
        r
    }

    fn roots(&self, s: &Shell) -> Vec<PathBuf> {
        let mut roots = vec![s.sessions.folder()];
        if let Ok(v) = s.obs.request(keystage_obs::ObsRequest::GetRecordDirectory) {
            if let Some(d) = v["recordDirectory"].as_str() {
                *self.record_dir.lock() = Some(PathBuf::from(d));
            }
        }
        if let Some(d) = self.record_dir.lock().clone() {
            roots.push(d);
        }
        roots
    }

    fn status(&self, s: &Shell) -> Value {
        let settings = s.settings();
        let monitors = s.monitors();
        json!({
            "app": "IDEA Keystage",
            "status": to_value(&s.status()),
            "monitors": to_value(&monitors),
            "virtual_screen": to_value(&os::virtual_screen()),
            "overlay_monitor": settings.host.monitor,
            "native_overlay": settings.host.native_overlay,
            "tutorial_capture": settings.session.tutorial_capture,
            "automation": {
                "connector": settings.automation.connector,
                "allow_input": settings.automation.allow_input,
                "running": self.running.load(Ordering::Acquire),
                "hooks_installed": s.hooks_installed(),
            },
        })
    }

    fn obs_status(&self, s: &Shell) -> Value {
        let settings = s.settings();
        let st = s.obs.status(settings.obs.enabled);
        let mut v = json!({"enabled": settings.obs.enabled, "obs": to_value(&st)});
        if !settings.obs.enabled {
            v["what_to_do"] = json!(OBS_OFF);
        } else if let Ok(d) = s.obs.request(keystage_obs::ObsRequest::GetRecordDirectory) {
            v["record_directory"] = d["recordDirectory"].clone();
        }
        v
    }

    fn file_info(&self, s: &Shell, path: &str) -> Result<Value, String> {
        let real = paths::allowed(path, &self.roots(s)).map_err(|e| format!("Refused: {e}."))?;
        let size = std::fs::metadata(&real)
            .map_err(|_| "The file could not be read.".to_string())?
            .len();
        let f =
            std::fs::File::open(&real).map_err(|_| "The file could not be read.".to_string())?;
        let digest = keystage_update::sums::digest_reader(f)
            .map_err(|_| "The file could not be read.".to_string())?;
        Ok(
            json!({"path": real.to_string_lossy(), "size": size, "sha256": keystage_update::sums::to_hex(&digest)}),
        )
    }

    fn read_chunk(&self, s: &Shell, a: ChunkArgs) -> Result<Value, String> {
        let real = paths::allowed(&a.path, &self.roots(s)).map_err(|e| format!("Refused: {e}."))?;
        let mut f =
            std::fs::File::open(&real).map_err(|_| "The file could not be read.".to_string())?;
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        let (offset, len) = paths::chunk_range(size, a.offset, a.length)
            .ok_or("The offset is past the end of the file.")?;
        f.seek(SeekFrom::Start(offset))
            .map_err(|_| "The file could not be read.".to_string())?;
        let mut buf = vec![0u8; len as usize];
        f.read_exact(&mut buf)
            .map_err(|_| "The file could not be read.".to_string())?;
        Ok(json!({
            "offset": offset,
            "length": len,
            "size": size,
            "eof": offset + len >= size,
            "base64": base64::engine::general_purpose::STANDARD.encode(&buf),
        }))
    }

    fn show_set(&self, s: &Shell, mode: &str) -> Result<Value, String> {
        let cur = s.settings();
        let now = [
            cur.overlay.callouts.enabled,
            cur.overlay.keyboard.enabled,
            cur.overlay.mouse.enabled,
            cur.overlay.cursor.enabled,
        ];
        let want = match mode {
            "auto" => match self.show_saved.lock().take() {
                Some(saved) => saved,
                None => now,
            },
            "all" => [true, true, true, true],
            "keys" => [true, false, false, false],
            "keyboard" => [false, true, false, false],
            "mouse" => [false, false, true, false],
            "cursor" => [false, false, false, true],
            _ => return Err("mode is auto, all, keys, keyboard, mouse or cursor.".into()),
        };
        if mode != "auto" {
            let mut saved = self.show_saved.lock();
            if saved.is_none() {
                *saved = Some(now);
            }
        }
        s.update_settings(|st| {
            st.overlay.callouts.enabled = want[0];
            st.overlay.keyboard.enabled = want[1];
            st.overlay.mouse.enabled = want[2];
            st.overlay.cursor.enabled = want[3];
        })?;
        Ok(json!({"callouts": want[0], "keyboard": want[1], "mouse": want[2], "cursor": want[3]}))
    }

    fn dispatch(&self, s: &Shell, tool: &str, a: &Value) -> Result<Value, String> {
        let input_tool = tools::INPUT_TOOLS.contains(&tool);
        if input_tool && !s.settings().automation.allow_input {
            return Err(keystage_automation::abort::AbortReason::NotAllowed
                .message()
                .into());
        }
        match tool {
            "status" => {
                args::<None0>(a)?;
                Ok(self.status(s))
            }
            "session_start" => {
                let x: StartArgs = args(a)?;
                let capture = x.capture.unwrap_or(s.settings().session.tutorial_capture);
                if !s.session_start_with(Origin::User, capture) {
                    return Err("A session log is already running. Stop it first.".into());
                }
                if let Some(n) = x.name.as_deref() {
                    s.add_marker_step(Some(n), None)?;
                }
                Ok(json!({"session": to_value(&s.sessions.lock().status()), "capture": capture}))
            }
            "session_stop" => {
                args::<None0>(a)?;
                let id = s
                    .sessions
                    .lock()
                    .status()
                    .map(|st| st.id)
                    .ok_or("No session log is running.")?;
                s.session_stop()?;
                let session = s.sessions.get(&id)?;
                Ok(to_value(&session))
            }
            "session_marker" => {
                let x: MarkerArgs = args(a)?;
                s.add_marker_step(Some(&x.name), x.step.as_deref())?;
                Ok(
                    json!({"session": to_value(&s.sessions.lock().status()), "t_ms": platform::now_ms()}),
                )
            }
            "session_list" => {
                let x: ListArgs = args(a)?;
                let mut list = s.sessions.list(platform::now_ms());
                list.truncate(x.limit.unwrap_or(20).clamp(1, 200));
                Ok(json!({"sessions": to_value(&list)}))
            }
            "session_get" => {
                let x: IdArgs = args(a)?;
                if !sessions::is_safe_id(&x.id) {
                    return Err("That is not a session id.".into());
                }
                Ok(to_value(&s.sessions.get(&x.id)?))
            }
            "obs_status" => {
                args::<None0>(a)?;
                Ok(self.obs_status(s))
            }
            "obs_start_recording" => {
                args::<None0>(a)?;
                s.obs_start_recording()?;
                Ok(self.obs_status(s))
            }
            "obs_stop_recording" => {
                args::<None0>(a)?;
                s.obs_stop_recording()
            }
            "overlay_set" => {
                let x: VisibleArgs = args(a)?;
                s.set_native_overlay(x.visible)?;
                Ok(json!({"native_overlay": x.visible}))
            }
            "show_set" => {
                let x: ModeArgs = args(a)?;
                self.show_set(s, &x.mode)
            }
            "profile_set" => {
                let x: IdArgs = args(a)?;
                let mode = match x.id.to_ascii_lowercase().as_str() {
                    "solidworks" => SolidworksMode::On,
                    "general" => SolidworksMode::Off,
                    "auto" => SolidworksMode::Auto,
                    _ => {
                        return Err("Choose solidworks, general or auto. A profile of your own follows its own app while the mode is auto.".into())
                    }
                };
                s.set_mode(mode)?;
                Ok(json!({"solidworks_mode": to_value(&mode)}))
            }
            "pause" | "resume" => {
                args::<None0>(a)?;
                s.set_paused(tool == "pause");
                Ok(json!({"paused": tool == "pause"}))
            }
            "recording_info" => {
                let x: PathArgs = args(a)?;
                self.file_info(s, &x.path)
            }
            "recording_read_chunk" => {
                let x: ChunkArgs = args(a)?;
                self.read_chunk(s, x)
            }
            "check_for_updates_now" => {
                args::<None0>(a)?;
                s.check_for_updates();
                Ok(to_value(&s.update_status()))
            }
            "foreground_window" => {
                args::<None0>(a)?;
                let (process, rect) =
                    os::foreground_window().ok_or("The window in front could not be read.")?;
                let monitor = sessions::monitor_at(
                    &s.monitors(),
                    rect.x + rect.width as i32 / 2,
                    rect.y + rect.height as i32 / 2,
                );
                Ok(json!({"process": process, "rect": to_value(&rect), "monitor": monitor}))
            }
            "occluders" => {
                let x: MonitorArgs = args(a)?;
                let (id, cap) = automation::capture_rect(s, x.monitor.as_deref())
                    .ok_or("No monitor could be read.")?;
                let occ = automation::occluders_now(&cap);
                Ok(
                    json!({"capture_monitor": id, "clean": occ.is_empty(), "occluders": to_value(&occ)}),
                )
            }
            "pointer_move" | "pointer_click" | "pointer_scroll" => {
                let scr = automation::screen(s);
                self.input(|| {
                    let mut r = Runner::start(s, None)?;
                    let _g = s.automated_guard();
                    let start = platform::now_ms();
                    let mut extra = json!({});
                    match tool {
                        "pointer_move" => {
                            let x: MoveArgs = args(a)?;
                            let t = Target {
                                x: x.x,
                                y: x.y,
                                monitor: x.monitor,
                            };
                            let (px, py) = scr.resolve(&t)?.ok_or("Give x and y.")?;
                            r.move_to(px, py, x.duration_ms)?;
                        }
                        "pointer_click" => {
                            let x: ClickArgs = args(a)?;
                            let t = Target {
                                x: x.x,
                                y: x.y,
                                monitor: x.monitor,
                            };
                            let at = scr.resolve(&t)?;
                            if let Some(o) =
                                r.click(at, x.button, x.double, x.wait_idle.as_ref())?
                            {
                                extra = json!({"idle_ms": o.waited_ms, "settled": o.settled});
                            }
                        }
                        _ => {
                            let x: ScrollArgs = args(a)?;
                            if !(1..=100).contains(&x.clicks) {
                                return Err("clicks is 1 to 100.".into());
                            }
                            let t = Target {
                                x: x.x,
                                y: x.y,
                                monitor: x.monitor,
                            };
                            let (px, py) = scr.resolve(&t)?.ok_or("Give x and y.")?;
                            r.scroll(px, py, x.clicks, x.direction)?;
                        }
                    }
                    let end = platform::now_ms();
                    let mut v =
                        json!({"start_ms": start, "end_ms": end, "cursor": os::cursor_pos()});
                    if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
                        m.extend(e.clone());
                    }
                    Ok(v)
                })
            }
            "pointer_drag" => {
                let x: DragArgs = args(a)?;
                let scr = automation::screen(s);
                if x.path.len() < 2 || x.path.len() > keystage_automation::script::MAX_DRAG_POINTS {
                    return Err("A drag needs 2 to 200 points.".into());
                }
                for p in &x.path {
                    scr.resolve(&Target {
                        x: Some(p[0]),
                        y: Some(p[1]),
                        monitor: None,
                    })?;
                }
                self.input(|| {
                    let mut r = Runner::start(s, None)?;
                    let _g = s.automated_guard();
                    let start = platform::now_ms();
                    r.drag(&x.path, x.button, x.duration_ms)?;
                    Ok(json!({"start_ms": start, "end_ms": platform::now_ms()}))
                })
            }
            "key_chord" | "type_text" => {
                let chord = if tool == "key_chord" {
                    Some(args::<KeysArgs>(a)?.keys)
                } else {
                    None
                };
                let text = if tool == "type_text" {
                    Some(args::<TextArgs>(a)?.text)
                } else {
                    None
                };
                self.input(|| {
                    let mut r = Runner::start(s, None)?;
                    let _g = s.automated_guard();
                    let start = platform::now_ms();
                    match (&chord, &text) {
                        (Some(k), _) => r.chord(k)?,
                        (_, Some(t)) => r.type_text(t)?,
                        _ => {}
                    }
                    Ok(json!({"start_ms": start, "end_ms": platform::now_ms()}))
                })
            }
            "wait_idle" => {
                let x: IdleArgs = args(a)?;
                if x.max_ms > keystage_automation::script::MAX_IDLE_MS {
                    return Err("max_ms is at most 30000.".into());
                }
                let start = std::time::Instant::now();
                let mut last = os::region_hash(&x.rect);
                let mut stable = std::time::Instant::now();
                loop {
                    let waited = start.elapsed().as_millis() as u64;
                    if last.is_some() && stable.elapsed().as_millis() >= 150 {
                        return Ok(json!({"idle_ms": waited, "settled": true}));
                    }
                    if waited >= x.max_ms {
                        return Ok(json!({"idle_ms": waited, "settled": false}));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    let h = os::region_hash(&x.rect);
                    if h != last || h.is_none() {
                        last = h;
                        stable = std::time::Instant::now();
                    }
                }
            }
            "perform" => {
                let script: Script = args(a)?;
                self.input(|| automation::perform(s, script))
            }
            _ => Err(format!("IDEA Keystage has no tool named {tool}.")),
        }
    }
}

impl keystage_server::Control for ControlService {
    fn token(&self) -> Option<String> {
        let s = self.shell.upgrade()?;
        if !s.settings().automation.connector {
            return None;
        }
        let t = self.token.lock().clone();
        token::is_token(&t).then_some(t)
    }

    fn tools(&self) -> Value {
        to_value(&tools::catalog())
    }

    fn call(&self, tool: &str, args: Value) -> Result<Value, String> {
        let Some(s) = self.shell.upgrade() else {
            return Err("IDEA Keystage is stopping.".into());
        };
        if s.is_quitting() {
            return Err("IDEA Keystage is stopping.".into());
        }
        let known = tools::catalog().iter().any(|t| t.name == tool);
        tracing::info!(
            tool = if known { tool } else { "unknown" },
            "connector call"
        );
        self.dispatch(&s, tool, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_control_file_keeps_its_token_and_follows_the_port() {
        let d = tempfile::tempdir().expect("d");
        let a = write_control_file(d.path(), 47820);
        // Off Windows there is no system random source in the stub; skip then.
        let Ok(a) = a else { return };
        assert!(token::is_token(&a));
        let b = write_control_file(d.path(), 47999).expect("again");
        assert_eq!(a, b, "the token stays");
        let f: ControlFile =
            serde_json::from_str(&std::fs::read_to_string(d.path().join(CONTROL_FILE)).expect("r"))
                .expect("j");
        assert_eq!(f.port, 47999);
        std::fs::write(
            d.path().join(CONTROL_FILE),
            "{\"port\":1,\"token\":\"weak\"}",
        )
        .expect("w");
        let c = write_control_file(d.path(), 1).expect("c");
        assert_ne!(c, "weak", "a weak token is replaced");
    }

    #[test]
    fn arguments_are_checked_strictly() {
        assert!(args::<None0>(&json!({})).is_ok());
        assert!(args::<None0>(&Value::Null).is_ok());
        assert!(args::<None0>(&json!({"x": 1})).is_err());
        let m: MarkerArgs = args(&json!({"name": "Sketch", "step": "s1"})).expect("m");
        assert_eq!(m.step.as_deref(), Some("s1"));
        assert!(args::<MarkerArgs>(&json!({})).is_err());
        assert!(OBS_OFF.contains("Connect to OBS"));
    }
}
