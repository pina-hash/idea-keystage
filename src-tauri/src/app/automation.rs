//! Runs the connector's input tools and `perform` (ARCHITECTURE.md section
//! 14): the plans from `keystage-automation` played through `SendInput`,
//! with the abort watch checked before every step.
//!
//! * One run at a time; while it runs `AppState.automated` is true (Host A
//!   shows "Automated").
//! * A person touching the mouse or keyboard, Escape, a lock, a pause,
//!   input capture stopping or the setting being turned off stops the run
//!   at once; typing stops before a password field or a hidden app.
//! * Whatever the run holds (a mouse button, modifiers) is released when it
//!   ends, however it ends.
//!
//! Privacy: injected input goes through the same hooks and the same rules
//! as a person's, so the overlay and the Session Log show it exactly like
//! typed input. Nothing here logs what is typed or where.

use std::time::{Duration, Instant};

use keystage_automation::abort::{Snapshot, Watch};
use keystage_automation::keys::{self, Key, KeyStep};
use keystage_automation::motion::{self, Point, Rect, Rng, Sample};
use keystage_automation::occlude;
use keystage_automation::script::{Action, Button, Direction, Idle, Screen, Script, Target};
use keystage_automation::spans::DirtySpans;
use serde_json::{json, Value};

use super::shell::Shell;
use super::state::Reason;
use crate::platform::{self, automation as os};

/// Settling before a press (the pointer stops, then the button goes down).
const SETTLE_MS: u64 = 120;
/// `wait_idle`: unchanged this long counts as idle.
const IDLE_STABLE_MS: u64 = 150;

/// Why a run ended early, as a sentence.
pub type Stop = String;

/// What `wait_idle` saw.
#[derive(Clone, Copy, Debug)]
pub struct IdleOutcome {
    pub waited_ms: u64,
    pub settled: bool,
}

/// The screen as the scripts see it.
pub fn screen(shell: &Shell) -> Screen {
    Screen {
        virtual_screen: os::virtual_screen(),
        monitors: shell
            .monitors()
            .into_iter()
            .map(|m| {
                (
                    m.id.clone(),
                    Rect {
                        x: m.x,
                        y: m.y,
                        width: m.width,
                        height: m.height,
                    },
                )
            })
            .collect(),
    }
}

/// The capture monitor: the one named, else the overlay's, else the primary.
pub fn capture_rect(shell: &Shell, named: Option<&str>) -> Option<(String, Rect)> {
    let ms = shell.monitors();
    let want = named
        .map(str::to_string)
        .or_else(|| shell.settings().host.monitor.clone());
    let m = want
        .and_then(|w| ms.iter().find(|m| m.id.eq_ignore_ascii_case(&w)).cloned())
        .or_else(|| ms.iter().find(|m| m.primary).cloned())
        .or_else(|| ms.first().cloned())?;
    Some((
        m.id.clone(),
        Rect {
            x: m.x,
            y: m.y,
            width: m.width,
            height: m.height,
        },
    ))
}

/// The occluders over the capture monitor now.
pub fn occluders_now(capture: &Rect) -> Vec<occlude::Occluder> {
    occlude::occluders(&os::top_level_windows(), capture)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Button(Button),
    Key(Key),
}

pub struct Runner<'a> {
    shell: &'a Shell,
    watch: Watch,
    rng: Rng,
    vscreen: Rect,
    held: Vec<Held>,
}

fn seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    t ^ u64::from(std::process::id()).rotate_left(32)
}

impl<'a> Runner<'a> {
    pub fn snapshot(shell: &Shell) -> Snapshot {
        let (events, escapes) = platform::human_input();
        let (paused, privacy) = shell.privacy_now();
        Snapshot {
            human_events: events,
            human_escapes: escapes,
            hooks_installed: shell.hooks_installed(),
            locked: privacy == Some(Reason::Locked),
            paused,
            password: privacy == Some(Reason::Password),
            hidden_app: matches!(privacy, Some(Reason::Excluded | Reason::Unidentified)),
        }
    }

    /// Starts a run, or says why it may not.
    pub fn start(shell: &'a Shell, seed_in: Option<u64>) -> Result<Runner<'a>, Stop> {
        let allowed = shell.settings().automation.allow_input;
        let watch =
            Watch::start(&Runner::snapshot(shell), allowed).map_err(|r| r.message().to_string())?;
        Ok(Runner {
            shell,
            watch,
            rng: Rng::new(seed_in.unwrap_or_else(seed)),
            vscreen: os::virtual_screen(),
            held: Vec::new(),
        })
    }

    fn check(&mut self, typing: bool) -> Result<(), Stop> {
        let allowed = self.shell.settings().automation.allow_input;
        let snap = Runner::snapshot(self.shell);
        self.watch
            .check(&snap, allowed, typing)
            .map_err(|r| r.message().to_string())
    }

    /// Sleeps `ms`, checking every 10 ms.
    pub fn sleep(&mut self, ms: u64) -> Result<(), Stop> {
        let end = Instant::now() + Duration::from_millis(ms);
        loop {
            self.check(false)?;
            let now = Instant::now();
            if now >= end {
                return Ok(());
            }
            std::thread::sleep((end - now).min(Duration::from_millis(10)));
        }
    }

    fn play(&mut self, samples: &[Sample]) -> Result<(), Stop> {
        let start = Instant::now();
        for s in samples {
            let at = start + Duration::from_millis(s.t_ms);
            let now = Instant::now();
            if at > now {
                std::thread::sleep(at - now);
            }
            self.check(false)?;
            os::move_to(s.x, s.y, &self.vscreen)
                .map_err(|_| "Windows did not accept the mouse movement.".to_string())?;
        }
        Ok(())
    }

    fn here(&self) -> Point {
        let (x, y) = os::cursor_pos().unwrap_or((0, 0));
        Point::new(f64::from(x), f64::from(y))
    }

    pub fn move_to(&mut self, x: i32, y: i32, duration_ms: Option<u64>) -> Result<(), Stop> {
        let seed = self.rng.next_u64();
        let path = motion::move_path(
            self.here(),
            Point::new(f64::from(x), f64::from(y)),
            duration_ms,
            seed,
        );
        self.play(&path)
    }

    fn press(&mut self, b: Button, down: bool) -> Result<(), Stop> {
        os::button(b, down).map_err(|_| "Windows did not accept the mouse button.".to_string())?;
        if down {
            self.held.push(Held::Button(b));
        } else {
            self.held.retain(|h| *h != Held::Button(b));
        }
        Ok(())
    }

    fn key(&mut self, k: Key, down: bool) -> Result<(), Stop> {
        os::key(k, down).map_err(|_| "Windows did not accept the key.".to_string())?;
        if down {
            self.held.push(Held::Key(k));
        } else {
            self.held.retain(|h| *h != Held::Key(k));
        }
        Ok(())
    }

    /// Waits until `rect` stops changing for 150 ms, at most `max_ms`.
    pub fn wait_idle(&mut self, rect: Rect, max_ms: u64) -> Result<IdleOutcome, Stop> {
        let start = Instant::now();
        let mut last = os::region_hash(&rect);
        let mut stable_since = Instant::now();
        loop {
            self.check(false)?;
            let waited = start.elapsed().as_millis() as u64;
            if stable_since.elapsed() >= Duration::from_millis(IDLE_STABLE_MS) && last.is_some() {
                return Ok(IdleOutcome {
                    waited_ms: waited,
                    settled: true,
                });
            }
            if waited >= max_ms {
                return Ok(IdleOutcome {
                    waited_ms: waited,
                    settled: false,
                });
            }
            std::thread::sleep(Duration::from_millis(30));
            let h = os::region_hash(&rect);
            if h != last || h.is_none() {
                last = h;
                stable_since = Instant::now();
            }
        }
    }

    /// The rectangle `wait_idle` watches by default: 64 by 64 around a point.
    pub fn idle_rect_around(x: i32, y: i32) -> Rect {
        Rect {
            x: x - 32,
            y: y - 32,
            width: 64,
            height: 64,
        }
    }

    pub fn click(
        &mut self,
        at: Option<(i32, i32)>,
        b: Button,
        double: bool,
        idle: Option<&Idle>,
    ) -> Result<Option<IdleOutcome>, Stop> {
        if let Some((x, y)) = at {
            self.move_to(x, y, None)?;
        }
        self.sleep(SETTLE_MS)?;
        let mut outcome = None;
        if let Some(i) = idle {
            let p = self.here();
            let rect = i
                .rect
                .unwrap_or_else(|| Runner::idle_rect_around(p.x as i32, p.y as i32));
            outcome = Some(self.wait_idle(rect, i.max_ms)?);
        }
        let times = if double { 2 } else { 1 };
        for n in 0..times {
            if n > 0 {
                let gap = self.rng.range_u64(90, 140);
                self.sleep(gap)?;
            }
            self.check(false)?;
            self.press(b, true)?;
            let hold = self.rng.range_u64(60, 110);
            self.sleep(hold)?;
            self.press(b, false)?;
        }
        Ok(outcome)
    }

    pub fn drag(
        &mut self,
        path: &[[f64; 2]],
        b: Button,
        duration_ms: Option<u64>,
    ) -> Result<(), Stop> {
        let pts: Vec<Point> = path.iter().map(|p| Point::new(p[0], p[1])).collect();
        let Some(first) = pts.first() else {
            return Ok(());
        };
        self.move_to(first.x.round() as i32, first.y.round() as i32, None)?;
        self.sleep(SETTLE_MS)?;
        self.press(b, true)?;
        let pause = self.rng.range_u64(60, 100);
        self.sleep(pause)?;
        let samples = motion::drag_path(&pts, duration_ms);
        self.play(&samples)?;
        let pause = self.rng.range_u64(60, 100);
        self.sleep(pause)?;
        self.press(b, false)
    }

    pub fn scroll(&mut self, x: i32, y: i32, clicks: u32, dir: Direction) -> Result<(), Stop> {
        self.move_to(x, y, None)?;
        self.sleep(80)?;
        let (delta, horizontal) = match dir {
            Direction::Up => (120, false),
            Direction::Down => (-120, false),
            Direction::Right => (120, true),
            Direction::Left => (-120, true),
        };
        for n in 0..clicks {
            if n > 0 {
                let gap = self.rng.range_u64(40, 90);
                self.sleep(gap)?;
            }
            self.check(false)?;
            os::wheel(delta, horizontal)
                .map_err(|_| "Windows did not accept the wheel.".to_string())?;
        }
        Ok(())
    }

    fn steps(&mut self, steps: &[KeyStep], typing: bool) -> Result<(), Stop> {
        for s in steps {
            self.sleep(s.delay_ms)?;
            self.check(typing && s.down)?;
            self.key(s.key, s.down)?;
        }
        Ok(())
    }

    pub fn chord(&mut self, text: &str) -> Result<(), Stop> {
        let steps = keys::chord(text, &mut self.rng).map_err(|e| e.to_string())?;
        // A shortcut is typing too: it never goes into a password field.
        self.steps(&steps, true)
    }

    pub fn type_text(&mut self, text: &str) -> Result<(), Stop> {
        let steps = keys::type_text(text, &mut self.rng).map_err(|e| e.to_string())?;
        self.check(true)?;
        self.steps(&steps, true)
    }
}

impl Drop for Runner<'_> {
    fn drop(&mut self) {
        // Release whatever is still down, newest first.
        for h in std::mem::take(&mut self.held).into_iter().rev() {
            let _ = match h {
                Held::Button(b) => os::button(b, false),
                Held::Key(k) => os::key(k, false),
            };
        }
    }
}

fn target(shell_screen: &Screen, t: &Target) -> Result<Option<(i32, i32)>, Stop> {
    shell_screen.resolve(t)
}

/// One action's timing in the result.
fn timing(
    index: usize,
    op: &str,
    start: u64,
    end: u64,
    video: &dyn Fn(u64) -> Option<u64>,
) -> Value {
    let mut v = json!({"index": index, "op": op, "start_ms": start, "end_ms": end});
    if let (Some(a), Some(b)) = (video(start), video(end)) {
        v["video_start_ms"] = json!(a);
        v["video_end_ms"] = json!(b);
    }
    v
}

/// `perform`: the whole take.
pub fn perform(shell: &Shell, s: Script) -> Result<Value, Stop> {
    let scr = screen(shell);
    let problems = keystage_automation::script::validate(&s, &scr);
    if !problems.is_empty() {
        return Err(format!("The script was not run. {}", problems.join("; ")));
    }
    let Some((cap_id, cap)) = capture_rect(shell, s.capture_monitor.as_deref()) else {
        return Err("No monitor could be read.".into());
    };
    let mut dirty = DirtySpans::default();
    let first = occluders_now(&cap);
    if s.require_clean && !first.is_empty() {
        return Err(format!(
            "Refused: these windows cover the capture monitor {cap_id}, so the take would not be clean. Ask the user to move or minimize them (IDEA Keystage never moves another app's window): {}",
            serde_json::to_string(&first).unwrap_or_default()
        ));
    }
    let mut runner = Runner::start(shell, s.seed)?;
    let _automated = shell.automated_guard();
    dirty.observe(platform::now_ms(), &first);
    tracing::info!(actions = s.actions.len(), "connector perform started");
    let mut done: Vec<(usize, &'static str, u64, u64, Value)> = Vec::new();
    let mut stopped: Option<String> = None;
    let mut obs_out = json!({});
    for (i, a) in s.actions.iter().enumerate() {
        let start = platform::now_ms();
        let mut extra = json!({});
        let r: Result<(), Stop> = (|| {
            match a {
                Action::Move { duration_ms, .. } => {
                    if let Some((x, y)) = target(&scr, &a.target().unwrap_or_default())? {
                        runner.move_to(x, y, *duration_ms)?;
                    }
                }
                Action::Click {
                    button,
                    double,
                    wait_idle,
                    ..
                } => {
                    let at = target(&scr, &a.target().unwrap_or_default())?;
                    if let Some(o) = runner.click(at, *button, *double, wait_idle.as_ref())? {
                        extra = json!({"idle_ms": o.waited_ms, "settled": o.settled});
                    }
                }
                Action::Drag {
                    path,
                    button,
                    duration_ms,
                } => runner.drag(path, *button, *duration_ms)?,
                Action::Scroll {
                    clicks, direction, ..
                } => {
                    if let Some((x, y)) = target(&scr, &a.target().unwrap_or_default())? {
                        runner.scroll(x, y, *clicks, *direction)?;
                    }
                }
                Action::Chord { keys } => runner.chord(keys)?,
                Action::Type { text } => runner.type_text(text)?,
                Action::Wait { ms } => runner.sleep(*ms)?,
                Action::WaitIdle { rect, max_ms } => {
                    let p = os::cursor_pos()
                        .unwrap_or((cap.x + cap.width as i32 / 2, cap.y + cap.height as i32 / 2));
                    let rect = rect.unwrap_or_else(|| Runner::idle_rect_around(p.0, p.1));
                    let o = runner.wait_idle(rect, *max_ms)?;
                    extra = json!({"idle_ms": o.waited_ms, "settled": o.settled});
                }
                Action::Marker { name, step } => {
                    shell.add_marker_step(Some(name), step.as_deref())?;
                    let occ = occluders_now(&cap);
                    dirty.observe(platform::now_ms(), &occ);
                    if !occ.is_empty() {
                        extra = json!({"occluders": occ});
                    }
                }
                Action::ObsStart => {
                    shell.obs_start_recording()?;
                }
                Action::ObsStop => {
                    obs_out = shell.obs_stop_recording()?;
                    extra = obs_out.clone();
                }
            }
            Ok(())
        })();
        let end = platform::now_ms();
        done.push((i, a.op(), start, end, extra));
        if let Err(e) = r {
            stopped = Some(e);
            break;
        }
    }
    drop(runner);
    let end = platform::now_ms();
    let video = shell.video_clock();
    let spans = dirty.finish(end, &video);
    let actions: Vec<Value> = done
        .iter()
        .map(|(i, op, a, b, extra)| {
            let mut v = timing(*i, op, *a, *b, &video);
            if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
                for (k, x) in e {
                    m.insert(k.clone(), x.clone());
                }
            }
            v
        })
        .collect();
    tracing::info!(completed = stopped.is_none(), "connector perform ended");
    Ok(json!({
        "completed": stopped.is_none(),
        "stopped": stopped,
        "actions": actions,
        "dirty_spans": spans,
        "capture_monitor": cap_id,
        "session": shell.sessions.lock().status().map(|s| s.id),
        "obs": obs_out,
    }))
}
