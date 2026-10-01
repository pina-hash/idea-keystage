//! Demo server: serves a built front end (`dist/`) and feeds it REAL
//! keystage-core Processor output from a scripted sequence of synthetic
//! input events. The screenshot and performance harnesses use it.
//!
//! ```text
//! cargo run -p keystage-server --example demo -- --port 47820 --dist dist \
//!     [--script solidworks|general|burst|idle] [--rate 20] [--duration 10] \
//!     [--perf] [--no-wait] [--once] [--seed 1] \
//!     [--state active|paused|password|excluded] [--enable keyboard,mouse,cursor]
//! ```
//!
//! * `solidworks`: shortcuts, single keys and middle mouse actions chosen
//!   from `data/profiles/solidworks.json` (through the core's built-in
//!   profile), a repeated shortcut, wheel notches and a typed value. Every
//!   caption comes from the data file through the Processor; this file
//!   names no SolidWorks command.
//! * `general`: shortcuts-only behavior with generic Windows keys and masked
//!   typing (no captions).
//! * `burst`: `--rate` modifier combos per second for `--duration` seconds
//!   (default 20 per second for 10 seconds); prints how many messages it
//!   sent.
//! * `idle`: no input; only the hello snapshot (empty and state screenshots).
//!
//! The script starts when the first overlay page says hello (`--no-wait`
//! starts it at once) and loops until `--duration` seconds have passed
//! (0, the default for `solidworks` and `general`, loops forever). `--once`
//! exits three seconds after the script ends.
//!
//! `--perf` sends `{"t":"perf","seq":n,"sent_ms":<Unix ms>,"id":<callout
//! id>,"count":<count>}` right before every callout message, so a page can
//! measure send-to-paint latency. The renderer ignores the message.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use keystage_core::keys::{self, CHARACTER_SCANS};
use keystage_core::legends::{build_legends, us_scan_to_vk};
use keystage_core::profile::Profile;
use keystage_core::protocol::{
    AppState, Legends, OverlayConfig, Route, ServerMsg, Suppressed, DEFAULT_PORT,
};
use keystage_core::{
    builtin, DisplayEvent, DisplayRules, InputEvent, KeyInput, MonitorRect, MouseButton, PhysKey,
    Processor, Suppress, UsLegends,
};
use keystage_server::{overlay_url, DirAssets, Hub, Server, ServerConfig};

const USAGE: &str = "usage: demo [--port 47820] [--dist dist] [--script solidworks|general|burst|idle] [--rate N] [--duration SECONDS] [--perf] [--no-wait] [--once] [--seed N] [--state active|paused|password|excluded] [--enable keyboard,mouse,cursor]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    SolidWorks,
    General,
    Burst,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StateMode {
    Active,
    Paused,
    Password,
    Excluded,
}

#[derive(Debug)]
struct Args {
    port: u16,
    dist: PathBuf,
    script: Script,
    rate: f64,
    duration: f64,
    perf: bool,
    wait: bool,
    once: bool,
    seed: u64,
    state: StateMode,
    enable: BTreeSet<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        port: DEFAULT_PORT,
        dist: PathBuf::from("dist"),
        script: Script::SolidWorks,
        rate: f64::NAN,
        duration: f64::NAN,
        perf: false,
        wait: true,
        once: false,
        seed: 1,
        state: StateMode::Active,
        enable: BTreeSet::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--port" => {
                a.port = value("--port")?
                    .parse()
                    .map_err(|_| "--port must be 0 to 65535".to_string())?
            }
            "--dist" => a.dist = PathBuf::from(value("--dist")?),
            "--script" => {
                a.script = match value("--script")?.as_str() {
                    "solidworks" => Script::SolidWorks,
                    "general" => Script::General,
                    "burst" => Script::Burst,
                    "idle" => Script::Idle,
                    other => return Err(format!("unknown script {other}")),
                }
            }
            "--rate" => {
                a.rate = value("--rate")?
                    .parse()
                    .ok()
                    .filter(|r: &f64| r.is_finite() && *r > 0.0 && *r <= 1000.0)
                    .ok_or("--rate must be a number above 0 and at most 1000")?
            }
            "--duration" => {
                a.duration = value("--duration")?
                    .parse()
                    .ok()
                    .filter(|d: &f64| d.is_finite() && *d >= 0.0)
                    .ok_or("--duration must be 0 or more seconds")?
            }
            "--seed" => {
                a.seed = value("--seed")?
                    .parse()
                    .map_err(|_| "--seed must be a whole number".to_string())?
            }
            "--state" => {
                a.state = match value("--state")?.as_str() {
                    "active" => StateMode::Active,
                    "paused" => StateMode::Paused,
                    "password" => StateMode::Password,
                    "excluded" => StateMode::Excluded,
                    other => return Err(format!("unknown state {other}")),
                }
            }
            "--enable" => {
                for part in value("--enable")?.split(',') {
                    let p = part.trim();
                    if !matches!(p, "keyboard" | "mouse" | "cursor") {
                        return Err(format!("--enable takes keyboard, mouse, cursor; got {p}"));
                    }
                    a.enable.insert(p.to_string());
                }
            }
            "--perf" => a.perf = true,
            "--no-wait" => a.wait = false,
            "--once" => a.once = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if a.rate.is_nan() {
        a.rate = if a.script == Script::Burst { 20.0 } else { 1.0 };
    }
    if a.duration.is_nan() {
        a.duration = if a.script == Script::Burst { 10.0 } else { 0.0 };
    }
    Ok(a)
}

/// One scripted input, `at` milliseconds after the script starts.
#[derive(Clone, Copy, Debug)]
enum Act {
    Key {
        vk: u16,
        scan: u16,
        ext: bool,
        down: bool,
    },
    Button {
        button: MouseButton,
        down: bool,
        x: i32,
        y: i32,
    },
    Move {
        x: i32,
        y: i32,
    },
    Wheel {
        dy: i32,
        x: i32,
        y: i32,
    },
}

/// US keyboard `(vk, scan, extended)` for canonical key names. Character
/// keys come from the core's own US tables.
struct Keyboard {
    keys: HashMap<String, (u16, u16, bool)>,
}

impl Keyboard {
    fn us() -> Keyboard {
        let mut keys = HashMap::new();
        for &scan in CHARACTER_SCANS {
            if let Some(vk) = us_scan_to_vk(scan) {
                let name = keys::classify(vk, PhysKey::new(scan, false)).name;
                keys.insert(name.into_owned(), (vk, scan, false));
            }
        }
        for n in 1u16..=12 {
            let scan = match n {
                1..=10 => 0x3A + n,
                11 => 0x57,
                _ => 0x58,
            };
            keys.insert(format!("F{n}"), (0x6F + n, scan, false));
        }
        let fixed: [(&str, u16, u16, bool); 20] = [
            ("Ctrl", 0xA2, 0x1D, false),
            ("Shift", 0xA0, 0x2A, false),
            ("Alt", 0xA4, 0x38, false),
            ("Win", 0x5B, 0x5B, true),
            ("Enter", 0x0D, 0x1C, false),
            ("Esc", 0x1B, 0x01, false),
            ("Tab", 0x09, 0x0F, false),
            ("Backspace", 0x08, 0x0E, false),
            ("Space", 0x20, 0x39, false),
            ("Delete", 0x2E, 0x53, true),
            ("Insert", 0x2D, 0x52, true),
            ("Home", 0x24, 0x47, true),
            ("End", 0x23, 0x4F, true),
            ("PageUp", 0x21, 0x49, true),
            ("PageDown", 0x22, 0x51, true),
            ("Up", 0x26, 0x48, true),
            ("Down", 0x28, 0x50, true),
            ("Left", 0x25, 0x4B, true),
            ("Right", 0x27, 0x4D, true),
            ("Menu", 0x5D, 0x5D, true),
        ];
        for (name, vk, scan, ext) in fixed {
            keys.insert(name.to_string(), (vk, scan, ext));
        }
        Keyboard { keys }
    }

    fn get(&self, name: &str) -> Option<(u16, u16, bool)> {
        self.keys.get(name).copied()
    }
}

/// Builds a timeline of acts.
struct Timeline<'k> {
    kb: &'k Keyboard,
    at: u64,
    acts: Vec<(u64, Act)>,
}

impl<'k> Timeline<'k> {
    fn new(kb: &'k Keyboard) -> Self {
        Timeline {
            kb,
            at: 0,
            acts: Vec::new(),
        }
    }

    fn push(&mut self, gap: u64, act: Act) {
        self.at += gap;
        self.acts.push((self.at, act));
    }

    fn key(&mut self, gap: u64, name: &str, down: bool) {
        let (vk, scan, ext) = self
            .kb
            .get(name)
            .unwrap_or_else(|| panic!("the demo keyboard has no key {name}"));
        self.push(
            gap,
            Act::Key {
                vk,
                scan,
                ext,
                down,
            },
        );
    }

    /// Modifiers down, key tapped, modifiers up in reverse order.
    fn combo(&mut self, keys: &[&str]) {
        let (mods, last) = keys.split_at(keys.len() - 1);
        for m in mods {
            self.key(25, m, true);
        }
        self.key(30, last[0], true);
        self.key(60, last[0], false);
        for m in mods.iter().rev() {
            self.key(25, m, false);
        }
    }

    fn type_text(&mut self, keys: &[&str]) {
        for k in keys {
            self.key(70, k, true);
            self.key(45, k, false);
        }
    }

    fn drag(&mut self, mods: &[&str], button: MouseButton, from: (i32, i32), to: (i32, i32)) {
        for m in mods {
            self.key(25, m, true);
        }
        self.push(
            40,
            Act::Button {
                button,
                down: true,
                x: from.0,
                y: from.1,
            },
        );
        let steps = 24;
        for i in 1..=steps {
            let x = from.0 + (to.0 - from.0) * i / steps;
            let y = from.1 + (to.1 - from.1) * i / steps;
            self.push(16, Act::Move { x, y });
        }
        self.push(
            30,
            Act::Button {
                button,
                down: false,
                x: to.0,
                y: to.1,
            },
        );
        for m in mods.iter().rev() {
            self.key(25, m, false);
        }
    }

    fn wheel(&mut self, notches: u32, dy: i32) {
        for _ in 0..notches {
            self.push(110, Act::Wheel { dy, x: 960, y: 540 });
        }
    }

    /// Moves the clock to the next action slot.
    fn pause_until(&mut self, at: u64) {
        self.at = self.at.max(at);
    }
}

/// Keyboard shortcuts from the data file the demo presses, in file order:
/// entries with no context whose last key is a letter. Combos use Ctrl,
/// Shift or both (at most three Ctrl, one Shift, one Ctrl + Shift); single
/// keys are letters (at most two). Keys whose entries the data marks as
/// general Windows functionality are skipped so the demo shows
/// SolidWorks-specific captions. The captions themselves come from the
/// Processor.
fn pick_shortcuts(profile: &Profile, kb: &Keyboard) -> (Vec<Vec<String>>, Vec<String>) {
    let windows_keys: BTreeSet<&Vec<String>> = profile
        .shortcuts
        .iter()
        .filter(|s| s.command.contains("Windows functionality"))
        .map(|s| &s.keys)
        .collect();
    let mut combos: Vec<Vec<String>> = Vec::new();
    let mut singles: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut per_mods: HashMap<Vec<String>, usize> = HashMap::new();
    for s in &profile.shortcuts {
        if !s.context.is_empty()
            || s.keys.is_empty()
            || windows_keys.contains(&s.keys)
            || !seen.insert(s.keys.clone())
        {
            continue;
        }
        let (mods, last) = s.keys.split_at(s.keys.len() - 1);
        let key = &last[0];
        let letter = key.len() == 1 && key.as_bytes()[0].is_ascii_uppercase();
        if !letter || kb.get(key).is_none() {
            continue;
        }
        if mods.is_empty() {
            if singles.len() < 2 {
                singles.push(key.clone());
            }
            continue;
        }
        let cap = match mods.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["Ctrl"] => 3,
            ["Shift"] | ["Ctrl", "Shift"] => 1,
            // Alt combos open menus and chords; Win is the system's.
            _ => 0,
        };
        let n = per_mods.entry(mods.to_vec()).or_insert(0);
        if *n < cap && combos.len() < 5 {
            *n += 1;
            combos.push(s.keys.clone());
        }
    }
    (combos, singles)
}

/// Middle-button drags from the data file: distinct modifier sets.
fn pick_drags(profile: &Profile) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for m in &profile.mouse {
        if m.button != "middle" || m.action != "drag" || m.direction.is_some() {
            continue;
        }
        if !m
            .modifiers
            .iter()
            .all(|x| matches!(x.as_str(), "Ctrl" | "Shift"))
        {
            continue;
        }
        if !out.contains(&m.modifiers) {
            out.push(m.modifiers.clone());
        }
        if out.len() == 3 {
            break;
        }
    }
    out
}

fn has_middle_wheel(profile: &Profile) -> bool {
    profile
        .mouse
        .iter()
        .any(|m| m.button == "middle" && m.action == "wheel" && m.modifiers.is_empty())
}

fn as_strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

fn solidworks_script(profile: &Profile, kb: &Keyboard, slot_ms: u64) -> Vec<(u64, Act)> {
    let (combos, singles) = pick_shortcuts(profile, kb);
    let drags = pick_drags(profile);
    let mut t = Timeline::new(kb);
    let mut slot = 0u64;
    let mut next = |t: &mut Timeline<'_>| {
        t.pause_until(slot * slot_ms);
        slot += 1;
    };
    let center = (960, 540);
    let mut combo_i = combos.iter();
    let mut single_i = singles.iter();
    let mut drag_i = drags.iter();
    if let Some(c) = combo_i.next() {
        next(&mut t);
        t.combo(&as_strs(c));
    }
    if let Some(k) = single_i.next() {
        next(&mut t);
        t.combo(&[k.as_str()]);
    }
    if let Some(d) = drag_i.next() {
        next(&mut t);
        t.drag(&as_strs(d), MouseButton::Middle, center, (1180, 640));
    }
    if let Some(c) = combo_i.next() {
        // Pressed three times within the repeat window: one callout, x3.
        next(&mut t);
        for _ in 0..3 {
            t.combo(&as_strs(c));
            t.at += 160;
        }
    }
    if has_middle_wheel(profile) {
        next(&mut t);
        t.wheel(4, 120);
    }
    if let Some(d) = drag_i.next() {
        next(&mut t);
        t.drag(&as_strs(d), MouseButton::Middle, (1100, 600), (820, 480));
    }
    // A typed value, as when entering a dimension.
    next(&mut t);
    t.type_text(&["2", "5", ".", "4"]);
    t.key(90, "Enter", true);
    t.key(50, "Enter", false);
    for c in combo_i.by_ref() {
        next(&mut t);
        t.combo(&as_strs(c));
    }
    if let Some(k) = single_i.next() {
        next(&mut t);
        t.combo(&[k.as_str()]);
    }
    if let Some(d) = drag_i.next() {
        next(&mut t);
        t.drag(&as_strs(d), MouseButton::Middle, center, (960, 380));
    }
    next(&mut t);
    t.acts
}

fn general_script(kb: &Keyboard, slot_ms: u64) -> Vec<(u64, Act)> {
    let mut t = Timeline::new(kb);
    let mut slot = 0u64;
    let mut next = |t: &mut Timeline<'_>| {
        t.pause_until(slot * slot_ms);
        slot += 1;
    };
    next(&mut t);
    t.combo(&["Ctrl", "C"]);
    next(&mut t);
    t.combo(&["Ctrl", "V"]);
    next(&mut t);
    // Typed text is masked: one "Typing" callout, no key identities.
    t.type_text(&["H", "E", "L", "L", "O", "Space", "W", "O", "R", "L", "D"]);
    next(&mut t);
    for _ in 0..3 {
        t.combo(&["Ctrl", "Z"]);
        t.at += 160;
    }
    next(&mut t);
    t.combo(&["Ctrl", "Shift", "N"]);
    next(&mut t);
    t.combo(&["Win", "Shift", "S"]);
    next(&mut t);
    t.combo(&["F5"]);
    next(&mut t);
    t.combo(&["Alt", "Tab"]);
    next(&mut t);
    t.combo(&["Esc"]);
    next(&mut t);
    t.acts
}

/// Small deterministic generator (xorshift64*), so a seed replays a burst.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn burst_script(kb: &Keyboard, rate: f64, duration_s: f64, seed: u64) -> (Vec<(u64, Act)>, u64) {
    let period = (1000.0 / rate).max(1.0);
    let presses = (rate * duration_s).round() as u64;
    let letters: Vec<String> = ('A'..='Z').map(|c| c.to_string()).collect();
    let mods: [&[&str]; 3] = [&["Ctrl"], &["Ctrl", "Shift"], &["Alt"]];
    let mut rng = Rng(seed.max(1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut acts = Vec::new();
    let span = (period * 0.6).max(4.0) as u64;
    for i in 0..presses {
        let start = (i as f64 * period) as u64;
        let m = mods[(rng.next() % 3) as usize];
        let k = &letters[(rng.next() % 26) as usize];
        let mut keys: Vec<&str> = m.to_vec();
        keys.push(k);
        // Spread the press over 60 percent of the period.
        let step = (span / (2 * keys.len() as u64)).max(1);
        let mut at = start;
        for name in &keys {
            let (vk, scan, ext) = kb.get(name).expect("burst keys exist");
            acts.push((
                at,
                Act::Key {
                    vk,
                    scan,
                    ext,
                    down: true,
                },
            ));
            at += step;
        }
        for name in keys.iter().rev() {
            let (vk, scan, ext) = kb.get(name).expect("burst keys exist");
            acts.push((
                at,
                Act::Key {
                    vk,
                    scan,
                    ext,
                    down: false,
                },
            ));
            at += step;
        }
    }
    (acts, presses)
}

#[derive(Default, Debug)]
struct Counts {
    messages: u64,
    callouts: u64,
    perf: u64,
}

struct Player {
    p: Processor,
    hub: Hub,
    start: Instant,
    perf: bool,
    counts: Counts,
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

impl Player {
    fn now_ms(&self) -> u64 {
        1_000 + self.start.elapsed().as_millis() as u64
    }

    fn emit(&mut self, out: Vec<DisplayEvent>) {
        for ev in out {
            if let DisplayEvent::Callout(c) = &ev {
                if self.perf {
                    self.counts.perf += 1;
                    let perf = format!(
                        r#"{{"t":"perf","seq":{},"sent_ms":{:.3},"id":{},"count":{}}}"#,
                        self.counts.perf,
                        unix_ms(),
                        c.id,
                        c.count
                    );
                    self.hub.publish(perf);
                    self.counts.messages += 1;
                }
                self.counts.callouts += 1;
            }
            self.hub.publish(ev.to_json());
            self.counts.messages += 1;
        }
    }

    /// Sleeps until `target` (processor ms), running `tick` when it is due.
    fn wait_until(&mut self, target: u64) {
        loop {
            let now = self.now_ms();
            if let Some(due) = self.p.next_tick_due() {
                if due <= now {
                    let out = self.p.tick(now);
                    self.emit(out);
                    continue;
                }
            }
            if now >= target {
                return;
            }
            let wake = self
                .p
                .next_tick_due()
                .map_or(target, |d| d.min(target))
                .max(now + 1);
            thread::sleep(Duration::from_millis((wake - now).min(5)));
        }
    }

    fn play(&mut self, act: Act) {
        let t_ms = self.now_ms();
        let ev = match act {
            Act::Key {
                vk,
                scan,
                ext,
                down,
            } => InputEvent::Key(KeyInput {
                vk,
                phys: PhysKey::new(scan, ext),
                down,
                injected: false,
                t_ms,
            }),
            Act::Button { button, down, x, y } => InputEvent::Button {
                button,
                down,
                x,
                y,
                injected: false,
                t_ms,
            },
            Act::Move { x, y } => InputEvent::Move { x, y, t_ms },
            Act::Wheel { dy, x, y } => InputEvent::Wheel {
                dx: 0,
                dy,
                x,
                y,
                t_ms,
            },
        };
        let out = self.p.handle(&ev, &UsLegends);
        self.emit(out);
    }
}

fn us_legends() -> Legends {
    build_legends(
        UsLegends::LAYOUT_ID,
        CHARACTER_SCANS
            .iter()
            .filter_map(|&s| us_scan_to_vk(s).map(|vk| (PhysKey::new(s, false), vk))),
        &UsLegends,
    )
}

fn run(args: Args) -> Result<(), String> {
    let assets = DirAssets::new(&args.dist).map_err(|e| {
        format!(
            "cannot read {}: {e}. Build the front end first (npm run build).",
            args.dist.display()
        )
    })?;
    if !assets.root().join("overlay.html").is_file() {
        return Err(format!(
            "{} has no overlay.html. Build the front end first (npm run build).",
            assets.root().display()
        ));
    }

    let kb = Keyboard::us();
    let sw = builtin::solidworks().map_err(|e| format!("the SolidWorks data file: {e}"))?;
    let profile = match args.script {
        Script::SolidWorks => sw.clone(),
        _ => builtin::general(),
    };

    let mut config = OverlayConfig::default();
    config.keyboard.enabled = args.enable.contains("keyboard");
    config.mouse.enabled = args.enable.contains("mouse");
    config.cursor.enabled = args.enable.contains("cursor");
    let mut state = AppState {
        profile: Some(profile.reference()),
        ..AppState::default()
    };
    let mut suppress = None;
    match args.state {
        StateMode::Active => {}
        StateMode::Paused => {
            state.paused = true;
            suppress = Some(Suppress::Paused);
        }
        StateMode::Password => {
            state.suppressed = Some(Suppressed::Password);
            suppress = Some(Suppress::Password);
        }
        StateMode::Excluded => {
            state.suppressed = Some(Suppressed::Excluded);
            suppress = Some(Suppress::ExcludedApp("demo.exe".into()));
        }
    }
    let hello = Arc::new(Mutex::new((config, state, us_legends())));

    let hub = Hub::new();
    {
        let hello = Arc::clone(&hello);
        hub.set_snapshot(move || {
            let guard = match hello.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            ServerMsg::hello(guard.0.clone(), guard.1.clone(), Some(guard.2.clone())).to_json()
        });
    }
    let server = Server::start(ServerConfig::new(args.port), Arc::new(assets), hub.clone())
        .map_err(|e| e.to_string())?;
    let port = server.port();
    println!("listening http://127.0.0.1:{port}");
    for r in [
        Route::Keys,
        Route::Keyboard,
        Route::Mouse,
        Route::Cursor,
        Route::All,
    ] {
        println!("  {}", overlay_url(port, r));
    }

    if args.script == Script::Idle {
        println!("idle: serving the hello snapshot only; press Ctrl+C to stop");
        if args.once {
            thread::sleep(Duration::from_secs_f64(args.duration.max(3.0)));
            return Ok(());
        }
        loop {
            thread::park();
        }
    }

    if args.wait {
        println!("waiting for an overlay page to connect");
        let mut counts = hub.watch_counts();
        loop {
            let c = *counts.borrow_and_update();
            if c.total > c.unidentified() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        // Give the page a moment to apply its hello before input starts.
        thread::sleep(Duration::from_millis(300));
        println!("overlay connected");
    }

    let mut p = Processor::new(DisplayRules::default());
    p.set_profile(Arc::new(profile));
    p.set_monitor(Some(MonitorRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    }));
    let _ = p.set_suppressed(suppress);
    let mut player = Player {
        p,
        hub: hub.clone(),
        start: Instant::now(),
        perf: args.perf,
        counts: Counts::default(),
    };

    let slot_ms = (1000.0 / args.rate).max(200.0) as u64;
    let (script, presses) = match args.script {
        Script::SolidWorks => (solidworks_script(&sw, &kb, slot_ms), 0),
        Script::General => (general_script(&kb, slot_ms), 0),
        Script::Burst => burst_script(&kb, args.rate, args.duration, args.seed),
        Script::Idle => (Vec::new(), 0),
    };
    let script_len = script.last().map_or(0, |(at, _)| *at) + 1_500;
    let limit_ms = if args.script == Script::Burst || args.duration == 0.0 {
        None
    } else {
        Some((args.duration * 1000.0) as u64)
    };
    let name = match args.script {
        Script::SolidWorks => "solidworks",
        Script::General => "general",
        Script::Burst => "burst",
        Script::Idle => "idle",
    };
    println!("script {name} started");
    let began = player.now_ms();
    let mut base = began;
    'outer: loop {
        for (at, act) in &script {
            if limit_ms.is_some_and(|l| base + at - began >= l) {
                break 'outer;
            }
            player.wait_until(base + at);
            player.play(*act);
        }
        if args.script == Script::Burst || limit_ms.is_some_and(|l| base + script_len - began >= l)
        {
            break;
        }
        base += script_len;
        player.wait_until(base);
    }
    // Let open runs and chords close.
    let settle = player.now_ms() + 1_500;
    player.wait_until(settle);
    let seconds = (player.now_ms() - began) as f64 / 1000.0;
    let c = &player.counts;
    if args.script == Script::Burst {
        println!(
            "burst: sent {} messages ({} callout messages for {} key presses{}) in {:.1} s",
            c.messages,
            c.callouts,
            presses,
            if args.perf {
                format!(", {} perf stamps", c.perf)
            } else {
                String::new()
            },
            seconds
        );
    } else {
        println!(
            "script {name} done: sent {} messages ({} callout messages) in {:.1} s",
            c.messages, c.callouts, seconds
        );
    }
    println!(
        "RESULT {{\"script\":\"{name}\",\"messages\":{},\"callouts\":{},\"perf\":{},\"presses\":{},\"seconds\":{:.3}}}",
        c.messages, c.callouts, c.perf, presses, seconds
    );
    if args.once {
        thread::sleep(Duration::from_secs(3));
        server.shutdown();
        return Ok(());
    }
    println!("still serving; press Ctrl+C to stop");
    loop {
        thread::park();
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("{msg}");
            }
            eprintln!("{USAGE}");
            return if msg.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("demo: {e}");
            ExitCode::FAILURE
        }
    }
}
