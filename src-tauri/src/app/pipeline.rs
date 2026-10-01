//! The pipeline thread (ARCHITECTURE.md section 1): it owns the
//! `keystage_core::Processor`, drains the platform's raw input and context
//! events, applies the privacy gate, picks the profile for the foreground
//! app, coalesces mouse moves, and hands every display event, serialized
//! once, to a [`Sink`] (Host A, the settings preview, the server hub, the
//! session log, the sound thread).
//!
//! Privacy (section 8):
//! * a password field, a locked session, an excluded app, or a foreground
//!   that could not be identified after a focus change suppresses
//!   everything (fail closed); pausing removes the hooks (the shell calls
//!   `InputHandle::pause`) and suppresses as well;
//! * while one of our own windows has the focus the general profile is used
//!   (typing is masked), whatever the SolidWorks mode says, so a password
//!   typed into our settings window can never be shown even if password
//!   detection misses it;
//! * nothing here logs input.
//!
//! The thread wakes when the hooks or the watcher unpark it and at least
//! every 16 ms to tick the processor and flush a coalesced mouse move. All
//! the decisions live in [`Core`], which is tested without threads.

use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use keystage_core::profile::{select_profile, GENERAL_ID};
use keystage_core::protocol::{Legends, ServerMsg, SolidworksMode};
use keystage_core::{
    DisplayEvent, InputEvent, MonitorRect, Processor, Profile, Settings, Suppress,
};

use super::state::{PipelineView, Reason};
use crate::platform::{
    self, ContextEvent, ForegroundInfo, ForegroundKind, InputHandle, LegendTable, ModifierSnapshot,
    WatcherHandle,
};

/// At most 125 mouse moves per second.
pub const MOVE_INTERVAL_MS: u64 = 8;
/// The longest the thread sleeps.
pub const PARK_MS: u64 = 16;
/// How often to tick when nothing else is due.
pub const TICK_EVERY_MS: u64 = 16;
/// Retry delay when a modifier resync could not read the keyboard.
pub const RESYNC_RETRY_MS: u64 = 500;

/// Messages from the shell to the pipeline.
#[derive(Debug)]
pub enum Control {
    Settings(Arc<Settings>),
    Paused(bool),
    Monitor(Option<MonitorRect>),
    Shutdown,
}

/// Where the pipeline's output goes.
pub trait Sink {
    /// One display event and its wire JSON (serialized once).
    fn display(&mut self, ev: &DisplayEvent, json: Arc<str>);
    /// The keyboard layout changed.
    fn legends(&mut self, legends: Legends, json: Arc<str>);
    /// The privacy reason or the profile changed.
    fn view(&mut self, view: PipelineView);
    /// The foreground window changed (re-assert the overlay's topmost).
    fn foreground_changed(&mut self);
}

/// What the foreground says about privacy.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Fg {
    /// An ordinary app (or one of ours).
    Normal,
    /// On the excluded list. The name stays inside the app.
    Excluded(String),
    /// Could not be identified after a focus change.
    Unidentified,
    /// An elevated window: the hooks are blind there, nothing to suppress.
    Elevated,
}

/// The privacy gate. Pause wins, then a locked session, a password field,
/// an excluded app, an unidentified foreground.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gate {
    paused: bool,
    password: bool,
    locked: bool,
    fg: Fg,
}

impl Default for Gate {
    fn default() -> Self {
        Gate {
            paused: false,
            password: false,
            locked: false,
            // Until the watcher reports the foreground, nothing is shown.
            fg: Fg::Unidentified,
        }
    }
}

impl Gate {
    /// The privacy reason, ignoring pause.
    pub fn privacy(&self) -> Option<Reason> {
        if self.locked {
            Some(Reason::Locked)
        } else if self.password {
            Some(Reason::Password)
        } else {
            match self.fg {
                Fg::Excluded(_) => Some(Reason::Excluded),
                Fg::Unidentified => Some(Reason::Unidentified),
                Fg::Normal | Fg::Elevated => None,
            }
        }
    }

    /// The processor's suppression.
    pub fn suppress(&self) -> Option<Suppress> {
        if self.paused {
            return Some(Suppress::Paused);
        }
        match self.privacy()? {
            Reason::Paused => Some(Suppress::Paused),
            Reason::Locked | Reason::Password => Some(Suppress::Password),
            Reason::Excluded => match &self.fg {
                Fg::Excluded(name) => Some(Suppress::ExcludedApp(name.clone())),
                _ => Some(Suppress::ExcludedApp(String::new())),
            },
            Reason::Unidentified => Some(Suppress::ExcludedApp(String::new())),
        }
    }
}

/// Keeps mouse moves to at most one per [`MOVE_INTERVAL_MS`].
#[derive(Clone, Debug, Default)]
pub struct MoveCoalescer {
    last_sent: Option<u64>,
    pending: Option<InputEvent>,
}

impl MoveCoalescer {
    fn spaced(&self, t: u64) -> bool {
        self.last_sent
            .is_none_or(|l| t >= l.saturating_add(MOVE_INTERVAL_MS))
    }

    /// A move arrived: returns it when it may go now, else keeps it.
    pub fn offer(&mut self, ev: InputEvent) -> Option<InputEvent> {
        let t = ev.t_ms();
        if self.spaced(t) {
            self.pending = None;
            self.last_sent = Some(t);
            Some(ev)
        } else {
            self.pending = Some(ev);
            None
        }
    }

    /// The kept move, once its slot has come.
    pub fn due(&mut self, now: u64) -> Option<InputEvent> {
        if self.pending.is_some() && self.spaced(now) {
            self.last_sent = Some(now);
            self.pending.take()
        } else {
            None
        }
    }

    /// Before a button or wheel event (which carry their own position): the
    /// kept move goes first when its slot has come, otherwise it is dropped.
    pub fn before_positioned(&mut self, now: u64) -> Option<InputEvent> {
        let due = self.due(now);
        self.pending = None;
        due
    }

    /// When the kept move may go.
    pub fn next_due(&self) -> Option<u64> {
        self.pending
            .as_ref()
            .map(|_| self.last_sent.unwrap_or(0).saturating_add(MOVE_INTERVAL_MS))
    }
}

/// The pipeline's state and decisions.
pub struct Core {
    processor: Processor,
    settings: Arc<Settings>,
    builtin: Arc<Profile>,
    general: Arc<Profile>,
    profiles: Vec<Arc<Profile>>,
    gate: Gate,
    own_foreground: bool,
    last_exe: Option<String>,
    legends: Arc<LegendTable>,
    caps_seeded: bool,
    coalescer: MoveCoalescer,
    last_tick: u64,
    next_resync: u64,
    out: Vec<DisplayEvent>,
    view: Option<PipelineView>,
}

impl Core {
    pub fn new(settings: Arc<Settings>, builtin: Arc<Profile>, legends: Arc<LegendTable>) -> Core {
        let processor = Processor::new(settings.display_rules());
        let profiles = settings.profiles.all_profiles(&builtin);
        let general = profiles
            .iter()
            .find(|p| p.id == GENERAL_ID)
            .cloned()
            .unwrap_or_else(|| Arc::new(Profile::general()));
        let mut core = Core {
            processor,
            settings,
            builtin,
            general,
            profiles,
            gate: Gate::default(),
            own_foreground: false,
            last_exe: None,
            legends,
            caps_seeded: false,
            coalescer: MoveCoalescer::default(),
            last_tick: 0,
            next_resync: 0,
            out: Vec::with_capacity(64),
            view: None,
        };
        core.select_profile();
        let s = core.gate.suppress();
        let _ = core.processor.set_suppressed(s);
        core
    }

    pub fn processor(&self) -> &Processor {
        &self.processor
    }

    pub fn gate(&self) -> &Gate {
        &self.gate
    }

    /// Handles a control message. Returns false on shutdown.
    pub fn on_control(&mut self, c: Control, now: u64, sink: &mut dyn Sink) -> bool {
        match c {
            Control::Settings(s) => {
                self.processor.set_rules(s.display_rules());
                self.profiles = s.profiles.all_profiles(&self.builtin);
                if let Some(g) = self.profiles.iter().find(|p| p.id == GENERAL_ID) {
                    self.general = g.clone();
                }
                self.settings = s;
                if matches!(self.gate.fg, Fg::Normal | Fg::Excluded(_)) {
                    if let Some(exe) = self.last_exe.clone() {
                        self.gate.fg = self.classify_exe(&exe);
                    }
                }
                self.select_profile();
                self.apply_gate(now);
            }
            Control::Paused(p) => {
                self.gate.paused = p;
                self.apply_gate(now);
            }
            Control::Monitor(m) => self.processor.set_monitor(m),
            Control::Shutdown => return false,
        }
        self.flush(sink);
        true
    }

    fn classify_exe(&self, exe: &str) -> Fg {
        if self.settings.privacy.is_excluded(exe) {
            Fg::Excluded(exe.to_string())
        } else {
            Fg::Normal
        }
    }

    fn select_profile(&mut self) {
        let chosen = if self.own_foreground {
            Some(self.general.clone())
        } else {
            select_profile(
                self.settings.profiles.solidworks_mode,
                self.last_exe.as_deref(),
                &self.profiles,
            )
            .cloned()
        };
        match chosen {
            Some(p) => self.processor.set_profile(p),
            None => self.processor.clear_profile(),
        }
    }

    fn apply_gate(&mut self, now: u64) {
        let want = self.gate.suppress();
        if self.processor.suppressed() != want.as_ref() {
            let evs = self.processor.set_suppressed(want);
            self.out.extend(evs);
        }
        let _ = now;
    }

    fn current_view(&self) -> PipelineView {
        PipelineView {
            privacy: self.gate.privacy(),
            profile: self.processor.profile().map(|p| p.reference()),
        }
    }

    /// Handles a watcher event.
    pub fn on_context(&mut self, ev: ContextEvent, now: u64, sink: &mut dyn Sink) {
        match ev {
            ContextEvent::Foreground(info) => {
                self.on_foreground(&info, now);
                sink.foreground_changed();
            }
            ContextEvent::Modifiers(snap) => self.on_modifiers(&snap),
            ContextEvent::Password { active } => {
                self.gate.password = active;
                self.apply_gate(now);
            }
            ContextEvent::Layout(table) => {
                self.legends = table;
                let wire = self.legends.wire();
                let json: Arc<str> = Arc::from(ServerMsg::Legends(wire.clone()).to_json());
                sink.legends(wire, json);
            }
            ContextEvent::SessionLocked { t_ms } => {
                let evs = self.processor.release_all(t_ms);
                self.out.extend(evs);
                self.gate.locked = true;
                self.apply_gate(now);
            }
            ContextEvent::SessionUnlocked { .. } => {
                self.gate.locked = false;
                self.apply_gate(now);
            }
        }
        self.flush(sink);
    }

    fn on_foreground(&mut self, info: &ForegroundInfo, now: u64) {
        match (info.kind, info.exe.as_deref()) {
            (ForegroundKind::App, Some(exe)) | (ForegroundKind::OwnProcess, Some(exe)) => {
                self.own_foreground = info.kind == ForegroundKind::OwnProcess;
                self.gate.fg = if self.own_foreground {
                    Fg::Normal
                } else {
                    self.classify_exe(exe)
                };
                self.last_exe = Some(exe.to_string());
                self.select_profile();
            }
            (ForegroundKind::OwnProcess, None) => {
                self.own_foreground = true;
                self.gate.fg = Fg::Normal;
                self.select_profile();
            }
            (ForegroundKind::Elevated, _) => {
                // The hooks see nothing there: show a neutral state.
                let evs = self.processor.release_all(now);
                self.out.extend(evs);
                self.gate.fg = Fg::Elevated;
            }
            (ForegroundKind::App, None) | (ForegroundKind::Unknown, _) => {
                // Keep the profile, hide everything until it is known again.
                self.gate.fg = Fg::Unidentified;
            }
        }
        self.apply_gate(now);
    }

    fn on_modifiers(&mut self, snap: &ModifierSnapshot) {
        if !self.caps_seeded {
            if let Some(c) = snap.caps_lock {
                self.processor.set_caps_lock(c);
                self.caps_seeded = true;
            }
        }
        let ev = snap.to_input_event();
        let legends: &LegendTable = &self.legends;
        self.processor.handle_into(&ev, legends, &mut self.out);
    }

    fn handle(&mut self, ev: &InputEvent) {
        let legends: &LegendTable = &self.legends;
        self.processor.handle_into(ev, legends, &mut self.out);
    }

    /// Handles drained hook events (the vector is emptied).
    pub fn on_input(&mut self, events: &mut Vec<InputEvent>, now: u64, sink: &mut dyn Sink) {
        for ev in events.drain(..) {
            match ev {
                InputEvent::Move { .. } => {
                    if let Some(m) = self.coalescer.offer(ev) {
                        self.handle(&m);
                    }
                }
                InputEvent::Button { .. } | InputEvent::Wheel { .. } => {
                    if let Some(m) = self.coalescer.before_positioned(now) {
                        self.handle(&m);
                    }
                    self.handle(&ev);
                }
                InputEvent::Key(_) | InputEvent::ModifierResync { .. } => self.handle(&ev),
            }
        }
        self.flush(sink);
    }

    /// Periodic work: a kept mouse move, the processor tick, a modifier
    /// resync when the processor asks for one.
    pub fn on_wake(
        &mut self,
        now: u64,
        sink: &mut dyn Sink,
        read_modifiers: &mut dyn FnMut() -> Option<ModifierSnapshot>,
    ) {
        if let Some(m) = self.coalescer.due(now) {
            self.handle(&m);
        }
        let tick_due = self.processor.next_tick_due().is_some_and(|t| t <= now);
        if tick_due || now >= self.last_tick.saturating_add(TICK_EVERY_MS) {
            let evs = self.processor.tick(now);
            self.out.extend(evs);
            self.last_tick = now;
        }
        if self.processor.needs_resync(now) && now >= self.next_resync {
            self.next_resync = now + RESYNC_RETRY_MS;
            if let Some(snap) = read_modifiers() {
                self.on_modifiers(&snap);
            }
        }
        self.flush(sink);
    }

    /// How long the thread may sleep.
    pub fn park_for(&self, now: u64) -> Duration {
        let mut until = now + PARK_MS;
        if let Some(t) = self.coalescer.next_due() {
            until = until.min(t);
        }
        if let Some(t) = self.processor.next_tick_due() {
            until = until.min(t);
        }
        Duration::from_millis(until.saturating_sub(now).clamp(1, PARK_MS))
    }

    fn flush(&mut self, sink: &mut dyn Sink) {
        for ev in self.out.drain(..) {
            let json: Arc<str> = Arc::from(ev.to_json());
            sink.display(&ev, json);
        }
        let view = self.current_view();
        if self.view.as_ref() != Some(&view) {
            self.view = Some(view.clone());
            sink.view(view);
        }
    }

    /// The SolidWorks mode the core runs with (for tests and diagnostics).
    pub fn mode(&self) -> SolidworksMode {
        self.settings.profiles.solidworks_mode
    }
}

/// The handles the pipeline thread started.
pub struct Started {
    pub input: Result<InputHandle, String>,
    pub watcher: Result<WatcherHandle, String>,
}

/// The shell's handle on the pipeline thread.
#[derive(Debug)]
pub struct PipelineHandle {
    tx: Sender<Control>,
    thread: Thread,
    join: parking_lot::Mutex<Option<JoinHandle<()>>>,
}

impl PipelineHandle {
    /// Sends a control message and wakes the thread.
    pub fn send(&self, c: Control) {
        if self.tx.send(c).is_ok() {
            self.thread.unpark();
        }
    }

    /// Stops the thread and waits for it.
    pub fn shutdown(&self) {
        self.send(Control::Shutdown);
        if let Some(j) = self.join.lock().take() {
            let _ = j.join();
        }
    }
}

/// Starts the pipeline thread. It starts the input hooks and the watcher
/// itself (they unpark it) and reports them through `on_started`.
pub fn spawn<S, F>(mut core: Core, mut sink: S, on_started: F) -> std::io::Result<PipelineHandle>
where
    S: Sink + Send + 'static,
    F: FnOnce(Started) + Send + 'static,
{
    let (tx, rx): (Sender<Control>, Receiver<Control>) = crossbeam_channel::unbounded();
    let join = thread::Builder::new()
        .name("keystage-pipeline".into())
        .spawn(move || {
            let me = thread::current();
            let (input, mut receiver) = match platform::start_input(me.clone()) {
                Ok((h, r)) => (Ok(h), Some(r)),
                Err(e) => {
                    tracing::error!(error = %e, "input hooks did not start");
                    (Err(e.to_string()), None)
                }
            };
            let (watcher, context) = match platform::start_watcher(me) {
                Ok((h, r)) => (Ok(h), Some(r)),
                Err(e) => {
                    tracing::error!(error = %e, "foreground watcher did not start");
                    (Err(e.to_string()), None)
                }
            };
            on_started(Started { input, watcher });
            tracing::info!("pipeline running");
            let mut buf: Vec<InputEvent> = Vec::with_capacity(256);
            let mut read = platform::read_modifiers;
            'run: loop {
                let now = platform::now_ms();
                while let Ok(c) = rx.try_recv() {
                    if !core.on_control(c, now, &mut sink) {
                        break 'run;
                    }
                }
                if let Some(ctx) = &context {
                    while let Ok(ev) = ctx.try_recv() {
                        core.on_context(ev, now, &mut sink);
                    }
                }
                if let Some(r) = receiver.as_mut() {
                    r.drain_into(&mut buf);
                    if !buf.is_empty() {
                        core.on_input(&mut buf, platform::now_ms(), &mut sink);
                    }
                }
                core.on_wake(platform::now_ms(), &mut sink, &mut read);
                thread::park_timeout(core.park_for(platform::now_ms()));
            }
            tracing::info!("pipeline stopped");
        })?;
    let thread = join.thread().clone();
    Ok(PipelineHandle {
        tx,
        thread,
        join: parking_lot::Mutex::new(Some(join)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::protocol::ProfileRef;
    use keystage_core::{builtin, KeyInput, Mod, PhysKey, Side};

    #[derive(Default)]
    struct Rec {
        shown: Vec<(DisplayEvent, String)>,
        legends: Vec<String>,
        views: Vec<PipelineView>,
        foreground: usize,
    }

    impl Sink for Rec {
        fn display(&mut self, ev: &DisplayEvent, json: Arc<str>) {
            self.shown.push((ev.clone(), json.to_string()));
        }
        fn legends(&mut self, _l: Legends, json: Arc<str>) {
            self.legends.push(json.to_string());
        }
        fn view(&mut self, v: PipelineView) {
            self.views.push(v);
        }
        fn foreground_changed(&mut self) {
            self.foreground += 1;
        }
    }

    impl Rec {
        fn callouts(&self) -> Vec<String> {
            self.shown
                .iter()
                .filter_map(|(e, _)| e.callout().map(|c| c.text_form()))
                .collect()
        }
        fn keys(&self) -> usize {
            self.shown
                .iter()
                .filter(|(e, _)| matches!(e, DisplayEvent::Key { .. }))
                .count()
        }
        fn moves(&self) -> usize {
            self.shown
                .iter()
                .filter(|(e, _)| matches!(e, DisplayEvent::Move { .. }))
                .count()
        }
        fn clear(&mut self) {
            self.shown.clear();
        }
    }

    fn core_with(settings: Settings) -> Core {
        let sw = Arc::new(builtin::solidworks().expect("profile"));
        Core::new(Arc::new(settings), sw, Arc::new(LegendTable::us()))
    }

    fn fg(kind: ForegroundKind, exe: Option<&str>) -> ContextEvent {
        ContextEvent::Foreground(ForegroundInfo {
            kind,
            exe: exe.map(str::to_string),
            pid: 42,
        })
    }

    fn key(vk: u16, scan: u16, ext: bool, down: bool, t: u64) -> InputEvent {
        InputEvent::Key(KeyInput {
            vk,
            phys: PhysKey::new(scan, ext),
            down,
            injected: false,
            t_ms: t,
        })
    }

    /// Ctrl + B, then a lone F5, then typing "ab" (masked in shortcuts mode).
    fn type_some(c: &mut Core, r: &mut Rec, t0: u64) {
        let mut evs = vec![
            key(0xA2, 0x1D, false, true, t0),
            key(0x42, 0x30, false, true, t0 + 10),
            key(0x42, 0x30, false, false, t0 + 20),
            key(0xA2, 0x1D, false, false, t0 + 30),
            key(0x74, 0x3F, false, true, t0 + 2000),
            key(0x74, 0x3F, false, false, t0 + 2010),
            key(0x41, 0x1E, false, true, t0 + 4000),
            key(0x41, 0x1E, false, false, t0 + 4010),
            key(0x42, 0x30, false, true, t0 + 4100),
            key(0x42, 0x30, false, false, t0 + 4110),
        ];
        c.on_input(&mut evs, t0 + 4200, r);
        let mut none = || None;
        c.on_wake(t0 + 4300, r, &mut none);
    }

    fn ready(c: &mut Core, r: &mut Rec, exe: &str) {
        c.on_context(fg(ForegroundKind::App, Some(exe)), 1, r);
        c.on_context(ContextEvent::Password { active: false }, 1, r);
        r.clear();
    }

    #[test]
    fn nothing_shows_before_the_foreground_is_known() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        type_some(&mut c, &mut r, 100);
        assert!(r.callouts().is_empty());
        assert_eq!(c.gate().privacy(), Some(Reason::Unidentified));
    }

    #[test]
    fn shortcuts_show_and_typing_is_masked_in_the_general_profile() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        type_some(&mut c, &mut r, 100);
        let callouts = r.callouts();
        assert!(callouts.contains(&"Ctrl + B".to_string()), "{callouts:?}");
        assert!(callouts.contains(&"F5".to_string()), "{callouts:?}");
        assert!(callouts.contains(&"Typing".to_string()), "{callouts:?}");
        // Masked letters never become key messages.
        assert!(!r.shown.iter().any(|(_, j)| j.contains("\"key\":\"A\"")));
        // The JSON handed on is exactly the event's wire form.
        for (ev, json) in &r.shown {
            assert_eq!(&ev.to_json(), json);
        }
        assert_eq!(
            r.views.last().and_then(|v| v.profile.clone()).map(|p| p.id),
            Some("general".to_string())
        );
    }

    #[test]
    fn a_password_field_hides_everything() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        c.on_context(ContextEvent::Password { active: true }, 50, &mut r);
        assert!(r
            .shown
            .iter()
            .any(|(e, _)| matches!(e, DisplayEvent::ModifiersReleased { .. })));
        assert_eq!(
            r.views.last().and_then(|v| v.privacy),
            Some(Reason::Password)
        );
        r.clear();
        type_some(&mut c, &mut r, 100);
        assert!(r.shown.is_empty(), "{:?}", r.shown);
        c.on_context(ContextEvent::Password { active: false }, 5000, &mut r);
        r.clear();
        type_some(&mut c, &mut r, 6000);
        assert!(!r.callouts().is_empty());
    }

    #[test]
    fn excluded_and_unidentified_apps_hide_everything() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "KeePass.exe");
        type_some(&mut c, &mut r, 100);
        assert!(r.shown.is_empty());
        assert_eq!(c.gate().privacy(), Some(Reason::Excluded));
        c.on_context(fg(ForegroundKind::App, Some("notepad.exe")), 5000, &mut r);
        r.clear();
        type_some(&mut c, &mut r, 6000);
        assert!(!r.callouts().is_empty());
        c.on_context(fg(ForegroundKind::Unknown, None), 11000, &mut r);
        r.clear();
        type_some(&mut c, &mut r, 12000);
        assert!(r.shown.is_empty());
        assert_eq!(
            r.views.last().and_then(|v| v.privacy),
            Some(Reason::Unidentified)
        );
        assert_eq!(
            c.gate().suppress(),
            Some(Suppress::ExcludedApp(String::new()))
        );
    }

    #[test]
    fn excluded_list_changes_apply_to_the_current_app() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        let mut s = Settings::default();
        s.privacy.excluded_apps.push("Notepad".into());
        c.on_control(Control::Settings(Arc::new(s)), 10, &mut r);
        assert_eq!(c.gate().privacy(), Some(Reason::Excluded));
        r.clear();
        type_some(&mut c, &mut r, 100);
        assert!(r.shown.is_empty());
    }

    #[test]
    fn pause_hides_everything_and_keeps_privacy_state() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        c.on_control(Control::Paused(true), 10, &mut r);
        assert_eq!(c.processor().suppressed(), Some(&Suppress::Paused));
        r.clear();
        type_some(&mut c, &mut r, 100);
        assert!(r.shown.is_empty());
        c.on_control(Control::Paused(false), 5000, &mut r);
        assert_eq!(c.processor().suppressed(), None);
    }

    #[test]
    fn a_locked_session_releases_keys_and_hides() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        let mut evs = vec![key(0xA2, 0x1D, false, true, 10)];
        c.on_input(&mut evs, 10, &mut r);
        r.clear();
        c.on_context(ContextEvent::SessionLocked { t_ms: 20 }, 20, &mut r);
        assert!(r
            .shown
            .iter()
            .any(|(e, _)| matches!(e, DisplayEvent::ModifiersReleased { .. })));
        assert_eq!(c.gate().privacy(), Some(Reason::Locked));
        c.on_context(ContextEvent::SessionUnlocked { t_ms: 30 }, 30, &mut r);
        assert_eq!(c.gate().privacy(), None);
    }

    fn profile_id(c: &Core) -> Option<String> {
        c.processor().profile().map(|p| p.id.clone())
    }

    #[test]
    fn the_profile_follows_the_foreground_and_the_mode() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "SLDWORKS.exe");
        assert_eq!(profile_id(&c).as_deref(), Some("solidworks"));
        c.on_context(fg(ForegroundKind::App, Some("notepad.exe")), 5, &mut r);
        assert_eq!(profile_id(&c).as_deref(), Some("general"));
        assert_eq!(
            r.views.last().and_then(|v| v.profile.clone()),
            Some(ProfileRef {
                id: "general".into(),
                name: "General".into()
            })
        );
        let mut on = Settings::default();
        on.profiles.solidworks_mode = SolidworksMode::On;
        c.on_control(Control::Settings(Arc::new(on.clone())), 6, &mut r);
        assert_eq!(c.mode(), SolidworksMode::On);
        assert_eq!(profile_id(&c).as_deref(), Some("solidworks"));
        // Our own window always uses the general profile (typing masked).
        c.on_context(
            fg(ForegroundKind::OwnProcess, Some("idea-keystage.exe")),
            7,
            &mut r,
        );
        assert_eq!(profile_id(&c).as_deref(), Some("general"));
        c.on_context(fg(ForegroundKind::App, Some("SLDWORKS.exe")), 8, &mut r);
        let mut off = Settings::default();
        off.profiles.solidworks_mode = SolidworksMode::Off;
        c.on_control(Control::Settings(Arc::new(off)), 9, &mut r);
        assert_eq!(profile_id(&c).as_deref(), Some("general"));
        // An unknown foreground keeps the profile.
        c.on_control(Control::Settings(Arc::new(on)), 10, &mut r);
        c.on_context(fg(ForegroundKind::Unknown, None), 11, &mut r);
        assert_eq!(profile_id(&c).as_deref(), Some("solidworks"));
        assert!(r.foreground >= 4);
    }

    #[test]
    fn solidworks_captions_come_from_the_data_file() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "SLDWORKS.exe");
        let mut evs = vec![
            key(0xA2, 0x1D, false, true, 100),
            key(0x42, 0x30, false, true, 110),
            key(0x42, 0x30, false, false, 120),
            key(0xA2, 0x1D, false, false, 130),
        ];
        c.on_input(&mut evs, 140, &mut r);
        let callout = r
            .shown
            .iter()
            .find_map(|(e, _)| e.callout().cloned())
            .expect("a callout");
        let data = builtin::solidworks().expect("profile");
        let documented: Vec<&str> = data
            .shortcuts
            .iter()
            .filter(|s| s.keys == ["Ctrl", "B"])
            .map(|s| s.command.as_str())
            .collect();
        match callout.caption.as_deref() {
            Some(cap) => assert!(documented.contains(&cap), "{cap} not in {documented:?}"),
            None => assert!(documented.is_empty()),
        }
        assert_eq!(callout.profile.as_deref(), Some("solidworks"));
    }

    #[test]
    fn mouse_moves_are_coalesced_to_125_per_second() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        c.on_control(
            Control::Monitor(Some(MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            })),
            0,
            &mut r,
        );
        let mut none = || None;
        // A 1000 Hz mouse for one second, drained in 4 ms batches.
        for batch in 0..250u64 {
            let mut evs: Vec<InputEvent> = (0..4u64)
                .map(|i| {
                    let t = 1000 + batch * 4 + i;
                    InputEvent::Move {
                        x: (t % 1900) as i32,
                        y: 500,
                        t_ms: t,
                    }
                })
                .collect();
            let now = 1000 + batch * 4 + 4;
            c.on_input(&mut evs, now, &mut r);
            c.on_wake(now, &mut r, &mut none);
        }
        let moves = r.moves();
        assert!((100..=126).contains(&moves), "{moves} moves");
        // The last position always arrives.
        c.on_wake(3000, &mut r, &mut none);
        let last = r
            .shown
            .iter()
            .rev()
            .find_map(|(e, _)| match e {
                DisplayEvent::Move { x, .. } => Some(*x),
                _ => None,
            })
            .expect("a move");
        // The last event was at t = 1999, x = 1999 % 1900 = 99.
        assert!((last - 99.0 / 1920.0).abs() < 1e-4, "{last}");
    }

    #[test]
    fn coalescer_rules() {
        let mv = |t| InputEvent::Move {
            x: 0,
            y: 0,
            t_ms: t,
        };
        let mut m = MoveCoalescer::default();
        assert!(m.offer(mv(0)).is_some());
        assert!(m.offer(mv(3)).is_none());
        assert_eq!(m.next_due(), Some(8));
        assert!(m.due(5).is_none());
        assert!(m.due(8).is_some());
        assert!(m.offer(mv(9)).is_none());
        assert!(
            m.before_positioned(10).is_none(),
            "dropped: the button has a position"
        );
        assert!(m.next_due().is_none());
        assert!(m.offer(mv(17)).is_some());
    }

    #[test]
    fn layouts_and_modifiers() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        c.on_context(ContextEvent::Layout(Arc::new(LegendTable::us())), 5, &mut r);
        assert_eq!(r.legends.len(), 1);
        assert!(r.legends[0].starts_with("{\"t\":\"legends\""));
        c.on_context(
            ContextEvent::Modifiers(ModifierSnapshot {
                held: vec![(Mod::Ctrl, Side::Left)],
                caps_lock: Some(false),
                t_ms: 6,
            }),
            6,
            &mut r,
        );
        assert!(r.keys() >= 1, "a held Ctrl lights after a resync");
    }

    #[test]
    fn park_time_is_bounded() {
        let c = core_with(Settings::default());
        let d = c.park_for(1000);
        assert!(d >= Duration::from_millis(1) && d <= Duration::from_millis(PARK_MS));
    }
}
