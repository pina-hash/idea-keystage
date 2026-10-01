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
//! * until UI Automation password detection runs (and if it stops), typing
//!   is masked in every profile ([`super::exposure`]), because only classic
//!   Edit password boxes would be detected;
//! * nothing here logs input.
//!
//! Order. The hook queue and the watcher's channel are separate, and the
//! watcher's events carry no time, so the thread cannot tell which keys
//! were typed before a context change and which after it. Each pass drains
//! the hook queue first and then reads the state changes, and
//! [`Core::pass`] resolves every doubt toward hiding: a change that shows
//! less (a password field, an excluded app, a lock) applies before the
//! drained input, and a change that shows more (leaving them, unlocking, a
//! profile that shows typed text) applies only after the input of this
//! pass and the next one, which hold everything typed before it.
//!
//! The thread wakes when the hooks or the watcher unpark it and at least
//! every 16 ms to tick the processor and flush a coalesced mouse move. All
//! the decisions live in [`Core`], which is tested without threads.
//!
//! Panics. Each pass runs under [`Guarded`]: a panic in the core or a sink
//! does not end the thread. The half-updated core is dropped, the hosts
//! release every key, and a new core is built from what the shell and the
//! watcher last sent, showing nothing until the watcher reports the
//! foreground again (fail closed). After [`MAX_RECOVERIES`] the thread
//! stops showing input for good but keeps draining the hook queue and
//! still obeys shutdown. The shell turns either into a notice.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use keystage_core::profile::{select_profile, GENERAL_ID};
use keystage_core::protocol::{Legends, ServerMsg, SolidworksMode};
use keystage_core::{
    DisplayEvent, DisplayRules, InputEvent, MonitorRect, Processor, Profile, Settings, Suppress,
};

use super::exposure::{self, Exposure};
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
/// Input batches handled under the stricter state before a change that
/// shows more takes effect: the batch drained before the change was read,
/// and the next one (see [`Core::pass`]).
pub const RELAX_AFTER_BATCHES: u8 = 2;
/// Panics the pipeline rebuilds its core after; the next one stops it.
pub const MAX_RECOVERIES: u32 = 3;

/// Messages from the shell to the pipeline.
#[derive(Debug)]
pub enum Control {
    Settings(Arc<Settings>),
    Paused(bool),
    Monitor(Option<MonitorRect>),
    Shutdown,
}

/// A state change read by one pass of the pipeline thread.
#[derive(Debug)]
pub enum Change {
    Control(Control),
    Context(ContextEvent),
    /// Whether UI Automation password detection runs
    /// (`WatcherHandle::uia_active`).
    PasswordDetection(bool),
}

/// What the processor should be set to.
struct Wanted {
    suppress: Option<Suppress>,
    profile: Option<Arc<Profile>>,
    rules: DisplayRules,
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
    /// A pass panicked. The core was rebuilt (`stopped` false) or the
    /// pipeline gave up showing input (`stopped` true).
    fn failed(&mut self, stopped: bool);
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
    /// The general rules last handed to the processor.
    applied_rules: DisplayRules,
    /// True while [`Core::pass`] applies its state changes.
    deferring: bool,
    /// Input batches still to handle under the stricter state in force
    /// before the wanted state, which shows more, is applied. 0: the
    /// processor is in the wanted state.
    relax_after: u8,
    /// UI Automation password detection runs. False until the watcher says
    /// so: typing is masked in every profile meanwhile.
    full_detection: bool,
    /// Masked variants of the profiles, built when first needed.
    masked: Vec<(Arc<Profile>, Arc<Profile>)>,
}

impl Core {
    /// A core that shows nothing until the watcher has reported the
    /// foreground, and masks typing until it reports that UI Automation
    /// password detection runs.
    pub fn new(settings: Arc<Settings>, builtin: Arc<Profile>, legends: Arc<LegendTable>) -> Core {
        let applied_rules = settings.display_rules();
        let processor = Processor::new(applied_rules.clone());
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
            applied_rules,
            deferring: false,
            relax_after: 0,
            full_detection: false,
            masked: Vec::new(),
        };
        let want = core.wanted();
        core.push(want);
        core.out.clear();
        core
    }

    pub fn processor(&self) -> &Processor {
        &self.processor
    }

    pub fn gate(&self) -> &Gate {
        &self.gate
    }

    /// One pass of the pipeline thread. `input` was drained from the hook
    /// queue before `changes` were read from their channels, so the order
    /// between the two is unknown: a change may have been sent before or
    /// after any of the drained keys. Every doubt is resolved toward hiding:
    ///
    /// * a change that shows the same or less applies before `input` is
    ///   handled (keys typed after it, in a password field or an excluded
    ///   app, are hidden);
    /// * a change that shows more (leaving a password field or an excluded
    ///   app, unlocking, resuming, a profile that shows typed text, password
    ///   detection back) waits: `input` is handled under the stricter state,
    ///   and so is the input of the next pass, because keys typed before the
    ///   change may still have been on their way into the queue when this
    ///   pass drained it. A key reaches the hook before the app that moves
    ///   the focus, and the watcher reports the move after it, so every key
    ///   typed before a change is drained by the pass after the one that
    ///   read the change. The cost is that keys typed within one pass (at
    ///   most [`PARK_MS`]) after such a change are hidden too.
    ///
    /// Returns false on shutdown.
    pub fn pass<I>(
        &mut self,
        input: &mut Vec<InputEvent>,
        changes: I,
        now: u64,
        sink: &mut dyn Sink,
    ) -> bool
    where
        I: IntoIterator<Item = Change>,
    {
        self.deferring = true;
        let mut running = true;
        for change in changes {
            match change {
                Change::Control(c) => {
                    if !self.on_control(c, now, sink) {
                        running = false;
                        break;
                    }
                }
                Change::Context(ev) => self.on_context(ev, now, sink),
                Change::PasswordDetection(full) => self.on_password_detection(full, sink),
            }
        }
        self.deferring = false;
        if !running {
            return false;
        }
        if !input.is_empty() {
            self.on_input(input, now, sink);
        }
        if self.relax_after > 0 {
            self.relax_after -= 1;
            if self.relax_after == 0 {
                let want = self.wanted();
                self.push(want);
            }
        }
        self.flush(sink);
        true
    }

    /// Handles a control message. Returns false on shutdown.
    pub fn on_control(&mut self, c: Control, _now: u64, sink: &mut dyn Sink) -> bool {
        match c {
            Control::Settings(s) => {
                self.profiles = s.profiles.all_profiles(&self.builtin);
                if let Some(g) = self.profiles.iter().find(|p| p.id == GENERAL_ID) {
                    self.general = g.clone();
                }
                self.masked.clear();
                self.settings = s;
                if matches!(self.gate.fg, Fg::Normal | Fg::Excluded(_)) {
                    if let Some(exe) = self.last_exe.clone() {
                        self.gate.fg = self.classify_exe(&exe);
                    }
                }
                self.apply();
            }
            Control::Paused(p) => {
                self.gate.paused = p;
                self.apply();
            }
            Control::Monitor(m) => self.processor.set_monitor(m),
            Control::Shutdown => return false,
        }
        self.flush(sink);
        true
    }

    /// UI Automation password detection started (`true`) or is not running.
    /// Without it, typing is masked in every profile.
    pub fn on_password_detection(&mut self, full: bool, sink: &mut dyn Sink) {
        if self.full_detection != full {
            self.full_detection = full;
            self.apply();
        }
        self.flush(sink);
    }

    /// True while UI Automation password detection is known to run.
    pub fn full_password_detection(&self) -> bool {
        self.full_detection
    }

    fn classify_exe(&self, exe: &str) -> Fg {
        if self.settings.privacy.is_excluded(exe) {
            Fg::Excluded(exe.to_string())
        } else {
            Fg::Normal
        }
    }

    /// The profile for the foreground.
    fn chosen_profile(&self) -> Option<Arc<Profile>> {
        if self.own_foreground {
            Some(self.general.clone())
        } else {
            select_profile(
                self.settings.profiles.solidworks_mode,
                self.last_exe.as_deref(),
                &self.profiles,
            )
            .cloned()
        }
    }

    fn masked_profile(&mut self, p: &Arc<Profile>) -> Arc<Profile> {
        if let Some((_, m)) = self.masked.iter().find(|(src, _)| Arc::ptr_eq(src, p)) {
            return m.clone();
        }
        let m = Arc::new(exposure::masked_profile(p));
        self.masked.push((p.clone(), m.clone()));
        m
    }

    /// The processor state the gate, the foreground, the settings and the
    /// password detection call for.
    fn wanted(&mut self) -> Wanted {
        let suppress = self.gate.suppress();
        let mut rules = self.settings.display_rules();
        let mut profile = self.chosen_profile();
        if !self.full_detection {
            rules = exposure::masked_rules(rules);
            profile = profile.map(|p| self.masked_profile(&p));
        }
        Wanted {
            suppress,
            profile,
            rules,
        }
    }

    fn applied_exposure(&self) -> Exposure {
        exposure::exposure(
            self.processor.suppressed().is_some(),
            self.processor.profile().map(|p| &**p),
            &self.applied_rules,
        )
    }

    /// Brings the processor in line with the wanted state. Inside a pass, a
    /// state that shows more than the one in force waits (see
    /// [`Core::pass`]); the stricter state stays until then.
    fn apply(&mut self) {
        let want = self.wanted();
        if self.deferring {
            let next = exposure::exposure(
                want.suppress.is_some(),
                want.profile.as_deref(),
                &want.rules,
            );
            if next > self.applied_exposure() {
                self.relax_after = RELAX_AFTER_BATCHES;
                return;
            }
        }
        self.push(want);
    }

    /// Sets the processor to `want` at once. Suppression starts before the
    /// profile changes and ends after it, so nothing is ever handled under
    /// a mix that shows more than either state.
    fn push(&mut self, want: Wanted) {
        let Wanted {
            suppress,
            profile,
            rules,
        } = want;
        let hiding = suppress.is_some();
        if hiding {
            self.set_suppressed(suppress.clone());
        }
        if rules != self.applied_rules {
            self.processor.set_rules(rules.clone());
            self.applied_rules = rules;
        }
        match profile {
            Some(p) => self.processor.set_profile(p),
            None => {
                if self.processor.profile().is_some() {
                    self.processor.clear_profile();
                }
            }
        }
        if !hiding {
            self.set_suppressed(None);
        }
        self.relax_after = 0;
    }

    fn set_suppressed(&mut self, s: Option<Suppress>) {
        if self.processor.suppressed() != s.as_ref() {
            let evs = self.processor.set_suppressed(s);
            self.out.extend(evs);
        }
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
                self.apply();
            }
            ContextEvent::Layout(table) => {
                self.legends = table;
                let wire = self.legends.wire();
                let json: Arc<str> = Arc::from(ServerMsg::Legends(wire.clone()).to_json());
                sink.legends(wire, json);
            }
            ContextEvent::SessionLocked { t_ms } => {
                self.gate.locked = true;
                let evs = self.processor.release_all(t_ms);
                self.out.extend(evs);
                self.apply();
            }
            ContextEvent::SessionUnlocked { .. } => {
                self.gate.locked = false;
                self.apply();
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
            }
            (ForegroundKind::OwnProcess, None) => {
                self.own_foreground = true;
                self.gate.fg = Fg::Normal;
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
        self.apply();
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

/// What the shell and the watcher last sent, recorded as each change is
/// read and before the core handles it, so a core rebuilt after a panic
/// starts from it even when the panic hit while that change was handled.
struct Seen {
    settings: Arc<Settings>,
    builtin: Arc<Profile>,
    legends: Arc<LegendTable>,
    monitor: Option<MonitorRect>,
    paused: bool,
    password: bool,
    locked: bool,
    shutdown: bool,
}

impl Seen {
    fn of(core: &Core) -> Seen {
        Seen {
            settings: core.settings.clone(),
            builtin: core.builtin.clone(),
            legends: core.legends.clone(),
            monitor: None,
            paused: core.gate.paused,
            password: core.gate.password,
            locked: core.gate.locked,
            shutdown: false,
        }
    }

    fn observe(&mut self, change: &Change) {
        match change {
            Change::Control(Control::Settings(s)) => self.settings = s.clone(),
            Change::Control(Control::Paused(p)) => self.paused = *p,
            Change::Control(Control::Monitor(m)) => self.monitor = *m,
            Change::Control(Control::Shutdown) => self.shutdown = true,
            Change::Context(ContextEvent::Password { active }) => self.password = *active,
            Change::Context(ContextEvent::SessionLocked { .. }) => self.locked = true,
            Change::Context(ContextEvent::SessionUnlocked { .. }) => self.locked = false,
            Change::Context(ContextEvent::Layout(t)) => self.legends = t.clone(),
            // The foreground is read again from the watcher after a rebuild;
            // password detection is reported on every pass.
            Change::Context(ContextEvent::Foreground(_) | ContextEvent::Modifiers(_))
            | Change::PasswordDetection(_) => {}
        }
    }

    /// A new core in the state last sent, except the foreground, which
    /// stays unidentified (nothing shows) until the watcher reports it, and
    /// password detection, which masks typing until the next pass says it
    /// runs.
    fn rebuild(&self, read_modifiers: &mut dyn FnMut() -> Option<ModifierSnapshot>) -> Core {
        let mut core = Core::new(
            self.settings.clone(),
            self.builtin.clone(),
            self.legends.clone(),
        );
        core.gate.paused = self.paused;
        core.gate.password = self.password;
        core.gate.locked = self.locked;
        core.processor.set_monitor(self.monitor);
        let want = core.wanted();
        core.push(want);
        if let Some(snap) = read_modifiers() {
            core.on_modifiers(&snap);
        }
        core.out.clear();
        core
    }
}

/// What the pipeline thread does after a [`Guarded::run`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Keep going.
    Run,
    /// A pass panicked and the core was rebuilt: ask the watcher to report
    /// the foreground again (`WatcherHandle::refresh`).
    Rebuilt,
    /// Shut down.
    Stop,
}

/// The core behind a panic guard (see the module comment).
pub struct Guarded {
    core: Option<Core>,
    seen: Seen,
    recoveries: u32,
}

impl Guarded {
    pub fn new(core: Core) -> Guarded {
        Guarded {
            seen: Seen::of(&core),
            core: Some(core),
            recoveries: 0,
        }
    }

    /// The running core; `None` once the pipeline gave up.
    pub fn core(&self) -> Option<&Core> {
        self.core.as_ref()
    }

    /// One pass of the thread: `gather` drains the hook queue into `input`
    /// and returns the state changes (read lazily), then [`Core::pass`] and
    /// [`Core::on_wake`] run. A panic anywhere in it is caught here.
    pub fn run<G, I>(
        &mut self,
        input: &mut Vec<InputEvent>,
        gather: G,
        now: &dyn Fn() -> u64,
        sink: &mut dyn Sink,
        read_modifiers: &mut dyn FnMut() -> Option<ModifierSnapshot>,
    ) -> Flow
    where
        G: FnOnce(&mut Vec<InputEvent>) -> I,
        I: IntoIterator<Item = Change>,
    {
        let Guarded { core, seen, .. } = self;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let changes = gather(&mut *input).into_iter().inspect(|c| seen.observe(c));
            match core.as_mut() {
                Some(core) => {
                    if !core.pass(input, changes, now(), &mut *sink) {
                        return false;
                    }
                    core.on_wake(now(), &mut *sink, &mut *read_modifiers);
                    true
                }
                None => {
                    // Gave up: the input is dropped, the changes only noted.
                    input.clear();
                    changes.for_each(drop);
                    !seen.shutdown
                }
            }
        }));
        match result {
            Ok(true) => Flow::Run,
            Ok(false) => Flow::Stop,
            Err(_) => {
                input.clear();
                if self.seen.shutdown {
                    return Flow::Stop;
                }
                self.recover(now(), sink, read_modifiers)
            }
        }
    }

    /// After a panic: drop the core, clear the hosts, rebuild or give up,
    /// tell the sink.
    fn recover(
        &mut self,
        now: u64,
        sink: &mut dyn Sink,
        read_modifiers: &mut dyn FnMut() -> Option<ModifierSnapshot>,
    ) -> Flow {
        self.core = None;
        self.recoveries += 1;
        if self.recoveries <= MAX_RECOVERIES {
            let seen = &self.seen;
            self.core = catch_unwind(AssertUnwindSafe(|| seen.rebuild(&mut *read_modifiers))).ok();
        }
        let stopped = self.core.is_none();
        tracing::error!(
            recoveries = self.recoveries,
            stopped,
            "pipeline pass failed, core dropped"
        );
        // Nothing stays lit on any host. A sink that panics again changes
        // nothing here.
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let ev = DisplayEvent::ModifiersReleased { t_ms: now };
            let json: Arc<str> = Arc::from(ev.to_json());
            sink.display(&ev, json);
            sink.failed(stopped);
        }));
        if stopped {
            Flow::Run
        } else {
            Flow::Rebuilt
        }
    }

    /// How long the thread may sleep.
    pub fn park_for(&self, now: u64) -> Duration {
        self.core
            .as_ref()
            .map_or(Duration::from_millis(PARK_MS), |c| c.park_for(now))
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
pub fn spawn<S, F>(core: Core, mut sink: S, on_started: F) -> std::io::Result<PipelineHandle>
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
            // Read on every pass: one atomic load.
            let detection = watcher.as_ref().ok().cloned();
            on_started(Started { input, watcher });
            tracing::info!("pipeline running");
            let mut buf: Vec<InputEvent> = Vec::with_capacity(256);
            let mut read = platform::read_modifiers;
            let mut guarded = Guarded::new(core);
            loop {
                let flow = guarded.run(
                    &mut buf,
                    |buf| {
                        // The input first, then the state changes (see
                        // `Core::pass`).
                        if let Some(r) = receiver.as_mut() {
                            r.drain_into(buf);
                        }
                        let full = detection.as_ref().is_some_and(WatcherHandle::uia_active);
                        rx.try_iter()
                            .map(Change::Control)
                            .chain(
                                context
                                    .iter()
                                    .flat_map(Receiver::try_iter)
                                    .map(Change::Context),
                            )
                            .chain(std::iter::once(Change::PasswordDetection(full)))
                    },
                    &platform::now_ms,
                    &mut sink,
                    &mut read,
                );
                match flow {
                    Flow::Run => {}
                    Flow::Stop => break,
                    Flow::Rebuilt => {
                        // The new core shows nothing until the foreground is
                        // reported again.
                        if let Some(w) = detection.as_ref() {
                            if let Err(e) = w.refresh() {
                                tracing::warn!(error = %e, "watcher refresh failed");
                            }
                        }
                    }
                }
                thread::park_timeout(guarded.park_for(platform::now_ms()));
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
        failures: Vec<bool>,
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
        fn failed(&mut self, stopped: bool) {
            self.failures.push(stopped);
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
        c.on_password_detection(true, r);
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

    /* --- Order between the hook queue and the state changes (Core::pass) -- */

    const VK_1: (u16, u16) = (0x31, 0x02);
    const VK_2: (u16, u16) = (0x32, 0x03);
    const VK_A: (u16, u16) = (0x41, 0x1E);
    const VK_S: (u16, u16) = (0x53, 0x1F);
    const VK_TAB: (u16, u16) = (0x09, 0x0F);

    /// Each key pressed and released, 100 ms apart, from `t0`.
    fn typed(t0: u64, keys: &[(u16, u16)]) -> Vec<InputEvent> {
        keys.iter()
            .enumerate()
            .flat_map(|(i, &(vk, scan))| {
                let t = t0 + i as u64 * 100;
                [
                    key(vk, scan, false, true, t),
                    key(vk, scan, false, false, t + 10),
                ]
            })
            .collect()
    }

    /// Alt + Tab from `t0`.
    fn alt_tab(t0: u64) -> Vec<InputEvent> {
        vec![
            key(0xA4, 0x38, false, true, t0),
            key(VK_TAB.0, VK_TAB.1, false, true, t0 + 10),
            key(VK_TAB.0, VK_TAB.1, false, false, t0 + 20),
            key(0xA4, 0x38, false, false, t0 + 30),
        ]
    }

    fn pass(c: &mut Core, r: &mut Rec, mut input: Vec<InputEvent>, changes: Vec<Change>, now: u64) {
        assert!(c.pass(&mut input, changes, now, r));
        assert!(input.is_empty(), "the batch is handled");
    }

    fn ctx(ev: ContextEvent) -> Change {
        Change::Context(ev)
    }

    fn password(active: bool) -> Change {
        ctx(ContextEvent::Password { active })
    }

    fn app(exe: &str) -> Change {
        ctx(fg(ForegroundKind::App, Some(exe)))
    }

    /// Nothing that reveals input: no callout, no key or button message.
    fn assert_nothing_revealed(r: &Rec) {
        let revealing: Vec<&String> = r
            .shown
            .iter()
            .filter(|(e, _)| !matches!(e, DisplayEvent::ModifiersReleased { .. }))
            .map(|(_, j)| j)
            .collect();
        assert!(revealing.is_empty(), "{revealing:?}");
    }

    fn texts(r: &Rec) -> Vec<String> {
        r.shown
            .iter()
            .filter_map(|(e, _)| e.callout())
            .filter_map(|c| c.text.clone())
            .collect()
    }

    fn solidworks_on() -> Settings {
        let mut s = Settings::default();
        s.profiles.solidworks_mode = SolidworksMode::On;
        s
    }

    #[test]
    fn keys_drained_with_the_end_of_a_password_field_stay_hidden() {
        // SolidWorks mode On: outside a password field, digits and letters
        // would show as text.
        let mut c = core_with(solidworks_on());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "chrome.exe");
        pass(&mut c, &mut r, vec![], vec![password(true)], 100);
        r.clear();
        // The pipeline was starved: the last characters and the Tab out of
        // the field are drained in the same pass that reads the verdict for
        // the next field.
        pass(
            &mut c,
            &mut r,
            typed(200, &[VK_1, VK_2, VK_A, VK_TAB]),
            vec![password(false)],
            700,
        );
        assert_nothing_revealed(&r);
        assert!(c.processor().suppressed().is_some());
        // Keys still on their way into the queue when that pass drained it
        // come with the next pass, which is still hidden.
        pass(&mut c, &mut r, typed(650, &[VK_2]), vec![], 710);
        assert_nothing_revealed(&r);
        // Then the field's state applies.
        assert_eq!(c.processor().suppressed(), None);
        assert_eq!(c.gate().privacy(), None);
        r.clear();
        pass(&mut c, &mut r, typed(3000, &[VK_1, VK_2]), vec![], 3300);
        assert_eq!(texts(&r).last().map(String::as_str), Some("12"));
    }

    #[test]
    fn the_last_keys_typed_in_an_excluded_app_stay_hidden_after_alt_tab() {
        let mut c = core_with(solidworks_on());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "KeePass.exe");
        assert_eq!(c.gate().privacy(), Some(Reason::Excluded));
        let mut input = typed(100, &[VK_1, VK_2, VK_A]);
        input.extend(alt_tab(500));
        pass(&mut c, &mut r, input, vec![app("notepad.exe")], 600);
        assert_nothing_revealed(&r);
        pass(&mut c, &mut r, vec![], vec![], 620);
        assert_nothing_revealed(&r);
        assert_eq!(c.processor().suppressed(), None);
        r.clear();
        pass(&mut c, &mut r, typed(2000, &[VK_1]), vec![], 2100);
        assert_eq!(texts(&r).last().map(String::as_str), Some("1"));
    }

    #[test]
    fn a_quick_trip_through_an_excluded_app_hides_what_was_typed_there() {
        let mut c = core_with(solidworks_on());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        // In and out of KeePass between two passes: its keys are drained
        // with both foreground changes.
        let mut input = alt_tab(100);
        input.extend(typed(200, &[VK_1, VK_2]));
        input.extend(alt_tab(400));
        pass(
            &mut c,
            &mut r,
            input,
            vec![app("KeePass.exe"), app("notepad.exe")],
            500,
        );
        assert_nothing_revealed(&r);
    }

    #[test]
    fn keys_typed_after_a_change_that_hides_are_hidden() {
        let mut c = core_with(solidworks_on());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        // The keys were typed in KeePass after the switch; the change is read
        // after they were drained, and still applies first.
        pass(
            &mut c,
            &mut r,
            typed(100, &[VK_1, VK_2]),
            vec![app("KeePass.exe")],
            300,
        );
        assert!(texts(&r).is_empty(), "{:?}", r.shown);
        assert!(r.callouts().is_empty());
        pass(&mut c, &mut r, vec![], vec![password(true)], 310);
        r.clear();
        pass(&mut c, &mut r, typed(400, &[VK_1]), vec![], 500);
        assert_nothing_revealed(&r);
    }

    #[test]
    fn typing_drained_with_a_switch_to_solidworks_stays_masked() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "notepad.exe");
        // Typed in Notepad (masked), drained with the switch to SolidWorks
        // (which shows typed values).
        pass(
            &mut c,
            &mut r,
            typed(100, &[VK_1, VK_2]),
            vec![app("SLDWORKS.exe")],
            300,
        );
        assert!(texts(&r).is_empty(), "{:?}", r.shown);
        assert!(r.callouts().contains(&"Typing".to_string()));
        assert_eq!(profile_id(&c).as_deref(), Some("general"));
        pass(&mut c, &mut r, vec![], vec![], 310);
        assert_eq!(profile_id(&c).as_deref(), Some("solidworks"));
        r.clear();
        pass(&mut c, &mut r, typed(3000, &[VK_1, VK_2]), vec![], 3300);
        assert_eq!(texts(&r).last().map(String::as_str), Some("12"));
    }

    #[test]
    fn removing_an_app_from_the_excluded_list_waits_for_its_keys() {
        let mut c = core_with(solidworks_on());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "KeePass.exe");
        let mut s = solidworks_on();
        s.privacy.excluded_apps.clear();
        pass(
            &mut c,
            &mut r,
            typed(100, &[VK_1, VK_2]),
            vec![Change::Control(Control::Settings(Arc::new(s)))],
            300,
        );
        assert_nothing_revealed(&r);
        assert_eq!(c.gate().privacy(), None);
        pass(&mut c, &mut r, vec![], vec![], 310);
        r.clear();
        pass(&mut c, &mut r, typed(3000, &[VK_1]), vec![], 3100);
        assert_eq!(texts(&r).last().map(String::as_str), Some("1"));
    }

    #[test]
    fn a_shutdown_ends_the_pass() {
        let mut c = core_with(Settings::default());
        let mut r = Rec::default();
        let mut input = typed(100, &[VK_1]);
        assert!(!c.pass(
            &mut input,
            vec![Change::Control(Control::Shutdown)],
            200,
            &mut r
        ));
    }

    #[test]
    fn without_ui_automation_typing_is_masked_in_every_profile() {
        let mut c = core_with(solidworks_on());
        let mut r = Rec::default();
        ready(&mut c, &mut r, "chrome.exe");
        // UI Automation is not running: a browser password field would not
        // be detected.
        c.on_password_detection(false, &mut r);
        assert!(!c.full_password_detection());
        r.clear();
        let mut input = typed(100, &[VK_1, VK_2, VK_S, VK_A]);
        input.extend([
            key(0xA2, 0x1D, false, true, 1000),
            key(0x42, 0x30, false, true, 1010),
            key(0x42, 0x30, false, false, 1020),
            key(0xA2, 0x1D, false, false, 1030),
        ]);
        c.on_input(&mut input, 1100, &mut r);
        assert!(texts(&r).is_empty(), "{:?}", r.callouts());
        let callouts = r.callouts();
        assert!(callouts.contains(&"Typing".to_string()), "{callouts:?}");
        // `S` and `A` are captioned bare keys in the SolidWorks list: they
        // stay masked too, and never become key messages.
        assert!(
            !callouts.iter().any(|c| c == "S" || c == "A"),
            "{callouts:?}"
        );
        assert!(!r
            .shown
            .iter()
            .any(|(_, j)| j.contains("\"key\":\"S\"") || j.contains("\"key\":\"1\"")));
        // Combos keep their captions.
        let combo = r
            .shown
            .iter()
            .filter_map(|(e, _)| e.callout())
            .find(|c| c.text_form() == "Ctrl + B")
            .expect("Ctrl + B shows");
        assert_eq!(combo.profile.as_deref(), Some("solidworks"));
        // Detection back: typed values show again.
        c.on_password_detection(true, &mut r);
        r.clear();
        let mut input = typed(5000, &[VK_1, VK_2]);
        c.on_input(&mut input, 5300, &mut r);
        assert_eq!(texts(&r).last().map(String::as_str), Some("12"));
    }

    #[test]
    fn a_new_core_masks_typing_until_detection_is_reported() {
        let mut c = core_with(solidworks_on());
        let mut r = Rec::default();
        assert!(!c.full_password_detection());
        c.on_context(app("chrome.exe").into_context(), 1, &mut r);
        c.on_context(ContextEvent::Password { active: false }, 1, &mut r);
        r.clear();
        let mut input = typed(100, &[VK_1, VK_2]);
        c.on_input(&mut input, 300, &mut r);
        assert!(texts(&r).is_empty());
        // Losing detection hides at once, getting it back waits a pass.
        pass(
            &mut c,
            &mut r,
            vec![],
            vec![Change::PasswordDetection(true)],
            400,
        );
        r.clear();
        pass(&mut c, &mut r, typed(500, &[VK_1]), vec![], 600);
        assert!(
            texts(&r).is_empty(),
            "drained with the change: still masked"
        );
        r.clear();
        pass(&mut c, &mut r, typed(3000, &[VK_1]), vec![], 3100);
        assert_eq!(texts(&r).last().map(String::as_str), Some("1"));
        r.clear();
        pass(
            &mut c,
            &mut r,
            typed(5000, &[VK_1]),
            vec![Change::PasswordDetection(false)],
            5100,
        );
        assert!(texts(&r).is_empty());
    }

    /* --- Panic guard ---------------------------------------------------- */

    /// A sink that panics on its next display event while armed.
    #[derive(Default)]
    struct Flaky {
        rec: Rec,
        armed: bool,
    }

    impl Sink for Flaky {
        fn display(&mut self, ev: &DisplayEvent, json: Arc<str>) {
            if std::mem::take(&mut self.armed) {
                panic!("test: the sink failed");
            }
            self.rec.display(ev, json);
        }
        fn legends(&mut self, l: Legends, json: Arc<str>) {
            self.rec.legends(l, json);
        }
        fn view(&mut self, v: PipelineView) {
            self.rec.view(v);
        }
        fn foreground_changed(&mut self) {
            self.rec.foreground_changed();
        }
        fn failed(&mut self, stopped: bool) {
            self.rec.failed(stopped);
        }
    }

    const VK_F5: (u16, u16) = (0x74, 0x3F);

    fn grun(
        g: &mut Guarded,
        s: &mut Flaky,
        input: Vec<InputEvent>,
        changes: Vec<Change>,
        now: u64,
    ) -> Flow {
        let mut buf = Vec::new();
        let flow = g.run(
            &mut buf,
            move |buf| {
                buf.extend(input);
                changes
            },
            &move || now,
            s,
            &mut || None,
        );
        assert!(buf.is_empty(), "nothing is left for the next pass");
        flow
    }

    /// Passes with password detection running (as the thread reports it).
    fn quiet(g: &mut Guarded, s: &mut Flaky, now: u64, n: u64) {
        for i in 0..n {
            let flow = grun(g, s, vec![], vec![Change::PasswordDetection(true)], now + i);
            assert_eq!(flow, Flow::Run);
        }
    }

    fn notepad() -> Change {
        ctx(fg(ForegroundKind::App, Some("notepad.exe")))
    }

    fn guarded_ready() -> (Guarded, Flaky) {
        let mut g = Guarded::new(core_with(Settings::default()));
        let mut s = Flaky::default();
        let flow = grun(
            &mut g,
            &mut s,
            vec![],
            vec![Change::PasswordDetection(true), notepad(), password(false)],
            1,
        );
        assert_eq!(flow, Flow::Run);
        quiet(&mut g, &mut s, 2, 3);
        s.rec.clear();
        (g, s)
    }

    fn panicking_pass(g: &mut Guarded, s: &mut Flaky) -> Flow {
        let mut buf = Vec::new();
        g.run(
            &mut buf,
            |_| -> Vec<Change> { panic!("test: the drain failed") },
            &|| 50,
            s,
            &mut || None,
        )
    }

    #[test]
    fn a_panic_rebuilds_the_core_releases_every_key_and_fails_closed() {
        let (mut g, mut s) = guarded_ready();
        grun(&mut g, &mut s, typed(100, &[VK_F5]), vec![], 300);
        assert!(s.rec.callouts().contains(&"F5".to_string()));
        s.rec.clear();

        s.armed = true;
        let flow = grun(&mut g, &mut s, typed(1000, &[VK_F5]), vec![], 1300);
        assert_eq!(flow, Flow::Rebuilt, "the thread survives");
        assert_eq!(s.rec.failures, vec![false]);
        assert!(
            matches!(
                s.rec.shown.last(),
                Some((DisplayEvent::ModifiersReleased { .. }, _))
            ),
            "the hosts release every key: {:?}",
            s.rec.shown
        );
        let core = g.core().expect("rebuilt");
        assert_eq!(core.gate().privacy(), Some(Reason::Unidentified));
        assert!(!core.full_password_detection(), "typing masked until told");
        s.rec.clear();

        // Nothing shows until the watcher reports the foreground again.
        grun(&mut g, &mut s, typed(2000, &[VK_F5]), vec![], 2300);
        assert!(s.rec.callouts().is_empty(), "{:?}", s.rec.shown);
        assert_eq!(s.rec.keys(), 0);

        grun(
            &mut g,
            &mut s,
            vec![],
            vec![Change::PasswordDetection(true), notepad()],
            3000,
        );
        quiet(&mut g, &mut s, 3001, 3);
        s.rec.clear();
        grun(&mut g, &mut s, typed(4000, &[VK_F5]), vec![], 4300);
        assert!(s.rec.callouts().contains(&"F5".to_string()));
    }

    #[test]
    fn a_change_read_in_the_failed_pass_survives_the_rebuild() {
        let (mut g, mut s) = guarded_ready();
        // The settings and the password field are read; showing the
        // suppression fails.
        s.armed = true;
        let flow = grun(
            &mut g,
            &mut s,
            vec![],
            vec![
                Change::Control(Control::Settings(Arc::new(solidworks_on()))),
                password(true),
                Change::PasswordDetection(true),
            ],
            100,
        );
        assert_eq!(flow, Flow::Rebuilt);
        let core = g.core().expect("rebuilt");
        assert_eq!(core.mode(), SolidworksMode::On);
        assert_eq!(core.gate().privacy(), Some(Reason::Password));

        // The foreground comes back; the password field still hides all.
        grun(
            &mut g,
            &mut s,
            vec![],
            vec![Change::PasswordDetection(true), notepad()],
            200,
        );
        quiet(&mut g, &mut s, 201, 3);
        let core = g.core().expect("running");
        assert_eq!(core.gate().privacy(), Some(Reason::Password));
        s.rec.clear();
        grun(&mut g, &mut s, typed(1000, &[VK_F5, VK_A]), vec![], 1300);
        assert!(s.rec.callouts().is_empty(), "{:?}", s.rec.shown);
        assert_eq!(s.rec.keys(), 0);
    }

    #[test]
    fn repeated_panics_stop_the_display_but_not_shutdown() {
        let (mut g, mut s) = guarded_ready();
        for _ in 0..MAX_RECOVERIES {
            assert_eq!(panicking_pass(&mut g, &mut s), Flow::Rebuilt);
        }
        assert_eq!(panicking_pass(&mut g, &mut s), Flow::Run);
        assert!(g.core().is_none(), "gave up");
        let mut want = vec![false; MAX_RECOVERIES as usize];
        want.push(true);
        assert_eq!(s.rec.failures, want);
        assert_eq!(g.park_for(0), Duration::from_millis(PARK_MS));
        s.rec.clear();

        // The queue is still drained, nothing shows.
        let flow = grun(
            &mut g,
            &mut s,
            typed(1000, &[VK_F5]),
            vec![Change::PasswordDetection(true), notepad(), password(false)],
            1300,
        );
        assert_eq!(flow, Flow::Run);
        quiet(&mut g, &mut s, 1301, 3);
        grun(&mut g, &mut s, typed(2000, &[VK_F5]), vec![], 2300);
        assert!(s.rec.shown.is_empty(), "{:?}", s.rec.shown);

        let flow = grun(
            &mut g,
            &mut s,
            vec![],
            vec![Change::Control(Control::Shutdown)],
            3000,
        );
        assert_eq!(flow, Flow::Stop);
    }

    impl Change {
        fn into_context(self) -> ContextEvent {
            match self {
                Change::Context(ev) => ev,
                other => panic!("not a context event: {other:?}"),
            }
        }
    }
}
