//! The obs-websocket client. It runs on its own thread with a
//! single-threaded tokio runtime, so the app drives it with plain method
//! calls and reads its state from a watch channel.
//!
//! Lifecycle: connect, read `Hello`, send `Identify` (RPC 1, authentication
//! when OBS asks, event subscriptions General | Outputs), wait for
//! `Identified`, then `GetVersion` (versions and whether
//! `CreateRecordChapter` exists) and `GetRecordStatus` (seeds the record
//! state). While a recording runs, `GetRecordStatus` is polled once a second
//! for the timecode and duration. Close code 4009 stops retrying until the
//! password changes; 4011 (kicked) and 4010 (unsupported) stop until the
//! app reconnects; anything else retries with backoff.
//!
//! The password is never logged; neither are chapter names or file paths.

use std::collections::HashMap;
use std::future::Future;
use std::net::Ipv6Addr;
use std::sync::mpsc as std_mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use keystage_core::ipc::ObsTestResult;
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::{interval, sleep, timeout, MissedTickBehavior};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::auth::{auth_string, Password};
use crate::protocol::{
    self, close_code, status_code, Incoming, RecordStatus, Version, CREATE_RECORD_CHAPTER,
    EVENT_SUBSCRIPTIONS, GET_RECORD_STATUS, GET_VERSION, RPC_VERSION,
};
use crate::status::{
    ChapterOutcome, ChapterSupport, Connection, DurationSample, ObsEvent, ObsStatus, RecordState,
    Recording,
};

/// obs-websocket's default address (README: "running by default on port
/// 4455"; Keystage's default host is the loopback address).
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 4455;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Where OBS is and the password the app decrypted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObsConfig {
    pub host: String,
    pub port: u16,
    pub password: Option<Password>,
}

impl Default for ObsConfig {
    fn default() -> Self {
        ObsConfig {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            password: None,
        }
    }
}

impl ObsConfig {
    /// An empty password counts as none.
    pub fn new(host: impl Into<String>, port: u16, password: Option<String>) -> ObsConfig {
        ObsConfig {
            host: host.into(),
            port,
            password: password.filter(|p| !p.is_empty()).map(Password::new),
        }
    }

    /// `ws://host:port`, or a UI message when the address is not usable.
    pub fn url(&self) -> Result<String, String> {
        let host = self.host.trim();
        let bad = || "The OBS address is not valid. Use a name like 127.0.0.1.".to_string();
        if self.port == 0 {
            return Err("The OBS port must be between 1 and 65535.".to_string());
        }
        if host.is_empty() || host.len() > 253 {
            return Err(bad());
        }
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        if bare.contains(':') {
            let ip: Ipv6Addr = bare.parse().map_err(|_| bad())?;
            return Ok(format!("ws://[{ip}]:{}", self.port));
        }
        let ok = host
            .split('.')
            .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
        if !ok {
            return Err(bad());
        }
        Ok(format!("ws://{host}:{}", self.port))
    }

    fn display_addr(&self) -> String {
        format!("{}:{}", self.host.trim(), self.port)
    }

    fn same_target(&self, other: &ObsConfig) -> bool {
        self.host.trim().eq_ignore_ascii_case(other.host.trim()) && self.port == other.port
    }
}

/// Timing knobs. The defaults are what the app uses; tests shorten them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientOptions {
    pub connect_timeout: Duration,
    /// How long to wait for any answer from OBS.
    pub request_timeout: Duration,
    /// `GetRecordStatus` interval while recording.
    pub poll_interval: Duration,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
}

impl Default for ClientOptions {
    fn default() -> Self {
        ClientOptions {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(5),
            poll_interval: Duration::from_secs(1),
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
        }
    }
}

type Reply = Box<dyn FnOnce(ChapterOutcome) + Send>;

enum Command {
    SetConfig(ObsConfig),
    Reconnect,
    Chapter { name: Option<String>, reply: Reply },
    Shutdown,
}

/// A running client. Dropping it disconnects and ends its thread.
pub struct ObsClient {
    cmd: mpsc::UnboundedSender<Command>,
    status: watch::Receiver<ObsStatus>,
    events: broadcast::Sender<ObsEvent>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ObsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObsClient")
            .field("status", &*self.status.borrow())
            .finish_non_exhaustive()
    }
}

impl ObsClient {
    /// Starts connecting with the default timings.
    pub fn spawn(config: ObsConfig) -> std::io::Result<ObsClient> {
        ObsClient::spawn_with(config, ClientOptions::default())
    }

    pub fn spawn_with(config: ObsConfig, options: ClientOptions) -> std::io::Result<ObsClient> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (status_tx, status_rx) = watch::channel(ObsStatus {
            connection: Connection::Connecting,
            ..ObsStatus::default()
        });
        let (events_tx, _) = broadcast::channel(256);
        let events = events_tx.clone();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let thread = thread::Builder::new()
            .name("keystage-obs".into())
            .spawn(move || {
                let ctx = Ctx {
                    opts: options,
                    status: status_tx,
                    events: events_tx,
                    cmds: cmd_rx,
                    rng: Jitter::seeded(),
                };
                rt.block_on(supervise(config, ctx));
            })?;
        Ok(ObsClient {
            cmd: cmd_tx,
            status: status_rx,
            events,
            thread: Some(thread),
        })
    }

    /// The current status.
    pub fn status(&self) -> ObsStatus {
        self.status.borrow().clone()
    }

    /// A watch on the status (connection, recording, chapter support).
    pub fn watch(&self) -> watch::Receiver<ObsStatus> {
        self.status.clone()
    }

    /// Record state changes, samples, chapter outcomes, in order.
    pub fn events(&self) -> broadcast::Receiver<ObsEvent> {
        self.events.subscribe()
    }

    /// New settings. Reconnects when the host, port or password changed.
    /// After an authentication failure only a different password (or
    /// address) retries.
    pub fn set_config(&self, config: ObsConfig) {
        let _ = self.cmd.send(Command::SetConfig(config));
    }

    /// Connects again now (after "kicked", or to skip a backoff wait).
    pub fn reconnect(&self) {
        let _ = self.cmd.send(Command::Reconnect);
    }

    /// Best effort: asks OBS to add a chapter marker; the outcome arrives as
    /// [`ObsEvent::Chapter`].
    pub fn add_chapter(&self, name: Option<&str>) {
        let events = self.events.clone();
        self.send_chapter(
            name,
            Box::new(move |o| {
                let _ = events.send(ObsEvent::Chapter(o));
            }),
        );
    }

    /// Asks OBS to add a chapter marker and waits for the outcome.
    pub fn create_chapter(&self, name: Option<&str>) -> impl Future<Output = ChapterOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send_chapter(
            name,
            Box::new(move |o| {
                let _ = tx.send(o);
            }),
        );
        async move { rx.await.unwrap_or(ChapterOutcome::NotConnected) }
    }

    /// Like [`ObsClient::create_chapter`] for a plain thread; gives
    /// [`ChapterOutcome::Timeout`] after `wait`.
    pub fn create_chapter_blocking(&self, name: Option<&str>, wait: Duration) -> ChapterOutcome {
        let (tx, rx) = std_mpsc::sync_channel(1);
        self.send_chapter(
            name,
            Box::new(move |o| {
                let _ = tx.send(o);
            }),
        );
        match rx.recv_timeout(wait) {
            Ok(o) => o,
            Err(std_mpsc::RecvTimeoutError::Timeout) => ChapterOutcome::Timeout,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => ChapterOutcome::NotConnected,
        }
    }

    fn send_chapter(&self, name: Option<&str>, reply: Reply) {
        let name = name
            .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|n| !n.is_empty());
        if let Err(mpsc::error::SendError(Command::Chapter { reply, .. })) =
            self.cmd.send(Command::Chapter { name, reply })
        {
            reply(ChapterOutcome::NotConnected);
        }
    }

    /// Disconnects and waits for the client thread to end.
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        let _ = self.cmd.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ObsClient {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Small xorshift for backoff jitter (no randomness crate needed).
struct Jitter(u64);

impl Jitter {
    fn seeded() -> Jitter {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Jitter(nanos | 1)
    }

    /// A factor between 0.8 and 1.2.
    fn factor(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        0.8 + (x % 10_001) as f64 / 10_000.0 * 0.4
    }
}

/// The delay before retry number `attempt` (0-based): the initial delay
/// doubling up to the maximum, then +/-20 percent.
fn backoff(opts: &ClientOptions, attempt: u32, jitter: &mut Jitter) -> Duration {
    let base = opts
        .backoff_initial
        .saturating_mul(2u32.saturating_pow(attempt.min(16)))
        .min(opts.backoff_max);
    base.mul_f64(jitter.factor())
}

struct Ctx {
    opts: ClientOptions,
    status: watch::Sender<ObsStatus>,
    events: broadcast::Sender<ObsEvent>,
    cmds: mpsc::UnboundedReceiver<Command>,
    rng: Jitter,
}

impl Ctx {
    fn set(&self, f: impl FnOnce(&mut ObsStatus)) {
        self.status.send_if_modified(|s| {
            let before = s.clone();
            f(s);
            *s != before
        });
    }

    fn emit(&self, e: ObsEvent) {
        let _ = self.events.send(e);
    }
}

/// How a session ended.
enum End {
    Shutdown,
    Changed(ObsConfig),
    Reconnect,
    /// Stop until the app changes something.
    Hold(Connection),
    /// Retry after a backoff.
    Retry {
        message: String,
        identified: bool,
    },
}

/// What woke a wait.
enum Wake {
    Shutdown,
    Changed(ObsConfig),
    Reconnect,
    Elapsed,
}

async fn supervise(mut config: ObsConfig, mut ctx: Ctx) {
    let mut attempt: u32 = 0;
    loop {
        if attempt == 0 {
            ctx.set(|s| {
                s.connection = Connection::Connecting;
                s.recording = None;
                s.chapters = ChapterSupport::Unknown;
            });
        }
        tracing::debug!(attempt, "connecting to obs-websocket");
        match run_session(&config, &mut ctx).await {
            End::Shutdown => break,
            End::Changed(c) => {
                config = c;
                attempt = 0;
            }
            End::Reconnect => attempt = 0,
            End::Hold(conn) => {
                tracing::info!(state = ?conn.ipc_state(), "obs-websocket connection stopped; waiting for new settings");
                let held = conn.clone();
                ctx.set(|s| {
                    s.connection = conn;
                    s.recording = None;
                });
                match hold(&config, &held, &mut ctx).await {
                    Wake::Shutdown => break,
                    Wake::Changed(c) => config = c,
                    Wake::Reconnect | Wake::Elapsed => {}
                }
                attempt = 0;
            }
            End::Retry {
                message,
                identified,
            } => {
                if identified {
                    attempt = 0;
                }
                ctx.set(|s| {
                    s.connection = Connection::Error(message);
                    s.recording = None;
                });
                let delay = backoff(&ctx.opts, attempt, &mut ctx.rng);
                attempt = attempt.saturating_add(1);
                tracing::debug!(
                    delay_ms = delay.as_millis() as u64,
                    "obs-websocket retry scheduled"
                );
                match wait(delay, &config, &mut ctx).await {
                    Wake::Shutdown => break,
                    Wake::Changed(c) => {
                        config = c;
                        attempt = 0;
                    }
                    Wake::Reconnect => attempt = 0,
                    Wake::Elapsed => {}
                }
            }
        }
    }
    ctx.set(|s| *s = ObsStatus::default());
    tracing::info!("obs-websocket client stopped");
}

/// Waits for a reason to try again after a stop (auth failure, kicked,
/// unsupported, bad address).
async fn hold(config: &ObsConfig, held: &Connection, ctx: &mut Ctx) -> Wake {
    loop {
        match ctx.cmds.recv().await {
            None | Some(Command::Shutdown) => return Wake::Shutdown,
            Some(Command::Reconnect) => return Wake::Reconnect,
            Some(Command::SetConfig(c)) => {
                let retry = match held {
                    // "No retry until the password changes" (or the address).
                    Connection::AuthFailed { .. } => {
                        c.password != config.password || !c.same_target(config)
                    }
                    _ => c != *config,
                };
                if retry {
                    return Wake::Changed(c);
                }
            }
            Some(Command::Chapter { reply, .. }) => reply(ChapterOutcome::NotConnected),
        }
    }
}

/// Sleeps `delay`, answering commands meanwhile.
async fn wait(delay: Duration, config: &ObsConfig, ctx: &mut Ctx) -> Wake {
    let sleeper = sleep(delay);
    tokio::pin!(sleeper);
    loop {
        tokio::select! {
            _ = &mut sleeper => return Wake::Elapsed,
            cmd = ctx.cmds.recv() => match cmd {
                None | Some(Command::Shutdown) => return Wake::Shutdown,
                Some(Command::Reconnect) => return Wake::Reconnect,
                Some(Command::SetConfig(c)) => {
                    if c != *config {
                        return Wake::Changed(c);
                    }
                }
                Some(Command::Chapter { reply, .. }) => reply(ChapterOutcome::NotConnected),
            },
        }
    }
}

/// Runs `fut` while answering commands; a command that ends the session
/// wins.
async fn with_commands<F: Future>(
    fut: F,
    config: &ObsConfig,
    ctx: &mut Ctx,
) -> Result<F::Output, End> {
    tokio::pin!(fut);
    loop {
        tokio::select! {
            out = &mut fut => return Ok(out),
            cmd = ctx.cmds.recv() => match cmd {
                None | Some(Command::Shutdown) => return Err(End::Shutdown),
                Some(Command::Reconnect) => return Err(End::Reconnect),
                Some(Command::SetConfig(c)) => {
                    if c != *config {
                        return Err(End::Changed(c));
                    }
                }
                Some(Command::Chapter { reply, .. }) => reply(ChapterOutcome::NotConnected),
            },
        }
    }
}

/// Why the handshake did not finish.
enum Fail {
    Hold(Connection),
    Retry(String),
}

impl Fail {
    fn into_end(self, identified: bool) -> End {
        match self {
            Fail::Hold(c) => End::Hold(c),
            Fail::Retry(message) => End::Retry {
                message,
                identified,
            },
        }
    }

    fn connection(self) -> Connection {
        match self {
            Fail::Hold(c) => c,
            Fail::Retry(m) => Connection::Error(m),
        }
    }
}

fn unsupported_rpc() -> Connection {
    Connection::Unsupported {
        reason: "This OBS does not speak obs-websocket version 5 (RPC 1), which Keystage needs. OBS 28 or later has it built in.".to_string(),
    }
}

/// Maps a close frame (or a lost connection) to what happens next.
fn closed(code: Option<u16>) -> Fail {
    match code {
        Some(close_code::AUTHENTICATION_FAILED) => Fail::Hold(Connection::AuthFailed {
            missing_password: false,
        }),
        Some(close_code::SESSION_INVALIDATED) => Fail::Hold(Connection::Kicked),
        Some(close_code::UNSUPPORTED_RPC_VERSION) => Fail::Hold(unsupported_rpc()),
        Some(1000) | Some(1001) | None => Fail::Retry("OBS closed the connection.".to_string()),
        Some(c) => Fail::Retry(format!("OBS closed the connection (code {c}).")),
    }
}

enum Read {
    Text(String),
    Closed(Option<u16>),
    Lost,
}

async fn read_text(ws: &mut Ws) -> Read {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => return Read::Text(t.as_str().to_owned()),
            Some(Ok(Message::Close(frame))) => {
                return Read::Closed(frame.map(|f| u16::from(f.code)))
            }
            Some(Ok(_)) => continue,
            Some(Err(_)) => return Read::Lost,
            None => return Read::Closed(None),
        }
    }
}

async fn send_text(ws: &mut Ws, text: String) -> bool {
    ws.send(Message::text(text)).await.is_ok()
}

async fn close_politely(ws: &mut Ws) {
    let _ = timeout(
        Duration::from_millis(500),
        ws.close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "".into(),
        })),
    )
    .await;
}

fn connect_error(e: &WsError, config: &ObsConfig) -> String {
    let addr = config.display_addr();
    match e {
        WsError::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => format!(
            "Could not reach OBS at {addr}. Open OBS and turn on Tools > WebSocket Server Settings > Enable WebSocket server."
        ),
        WsError::Io(_) => format!("Could not reach OBS at {addr}."),
        WsError::Http(resp) => format!(
            "{addr} answered, but not as obs-websocket (HTTP {}).",
            resp.status().as_u16()
        ),
        _ => format!("Could not open a WebSocket to OBS at {addr}."),
    }
}

/// What the handshake learned.
struct Hello {
    obs_version: String,
    ws_version: String,
}

/// Connect, Hello, Identify, Identified.
async fn handshake(
    config: &ObsConfig,
    opts: &ClientOptions,
    subscriptions: u32,
) -> Result<(Ws, Hello), Fail> {
    let url = config.url().map_err(|m| Fail::Hold(Connection::Error(m)))?;
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(16 << 20))
        .max_frame_size(Some(16 << 20));
    let (mut ws, _) = match timeout(
        opts.connect_timeout,
        tokio_tungstenite::connect_async_with_config(url, Some(ws_config), true),
    )
    .await
    {
        Err(_) => {
            return Err(Fail::Retry(format!(
                "OBS did not answer at {}.",
                config.display_addr()
            )))
        }
        Ok(Err(e)) => return Err(Fail::Retry(connect_error(&e, config))),
        Ok(Ok(pair)) => pair,
    };
    let no_answer = || Fail::Retry(format!("OBS did not answer at {}.", config.display_addr()));
    let hello = match timeout(opts.request_timeout, read_text(&mut ws)).await {
        Err(_) => return Err(no_answer()),
        Ok(Read::Text(t)) => t,
        Ok(Read::Closed(code)) => return Err(closed(code)),
        Ok(Read::Lost) => return Err(closed(None)),
    };
    let Some(Incoming::Hello {
        obs_version,
        ws_version,
        rpc_version,
        auth,
    }) = protocol::parse(&hello)
    else {
        return Err(Fail::Retry(format!(
            "{} answered, but not as obs-websocket 5.",
            config.display_addr()
        )));
    };
    if rpc_version < RPC_VERSION {
        close_politely(&mut ws).await;
        return Err(Fail::Hold(unsupported_rpc()));
    }
    let authentication = match (auth, &config.password) {
        (None, _) => None,
        (Some((challenge, salt)), Some(pw)) if !pw.is_empty() => {
            Some(auth_string(pw.expose(), &salt, &challenge))
        }
        (Some(_), _) => {
            close_politely(&mut ws).await;
            return Err(Fail::Hold(Connection::AuthFailed {
                missing_password: true,
            }));
        }
    };
    if !send_text(
        &mut ws,
        protocol::identify(authentication.as_deref(), subscriptions),
    )
    .await
    {
        return Err(closed(None));
    }
    drop(authentication);
    loop {
        match timeout(opts.request_timeout, read_text(&mut ws)).await {
            Err(_) => return Err(no_answer()),
            Ok(Read::Text(t)) => match protocol::parse(&t) {
                Some(Incoming::Identified { .. }) => break,
                // Nothing else is sent before Identified; skip noise.
                _ => continue,
            },
            Ok(Read::Closed(code)) => return Err(closed(code)),
            Ok(Read::Lost) => return Err(closed(None)),
        }
    }
    Ok((
        ws,
        Hello {
            obs_version: obs_version.unwrap_or_default(),
            ws_version: ws_version.unwrap_or_default(),
        },
    ))
}

enum Pending {
    Version,
    Status { sent: Instant },
    Chapter { reply: Reply },
}

/// State of one identified session.
struct Live {
    pending: HashMap<String, (Instant, Pending)>,
    next_id: u64,
    announced: bool,
    hello: Hello,
    recording: Recording,
    /// True once the record state came from OBS (a seed or an event).
    seeded: bool,
    chapters: ChapterSupport,
    status_outstanding: bool,
}

impl Live {
    fn id(&mut self) -> String {
        self.next_id += 1;
        format!("ks-{}", self.next_id)
    }

    fn publish(&self, ctx: &Ctx) {
        if !self.announced {
            return;
        }
        let rec = self.recording.clone();
        let chapters = self.chapters;
        ctx.set(|s| {
            s.recording = Some(rec);
            s.chapters = chapters;
        });
    }

    fn fail_pending(&mut self) {
        for (_, (_, p)) in self.pending.drain() {
            if let Pending::Chapter { reply } = p {
                reply(ChapterOutcome::NotConnected);
            }
        }
    }
}

async fn send_request(
    ws: &mut Ws,
    live: &mut Live,
    kind: &str,
    data: Option<Value>,
    p: Pending,
) -> bool {
    let id = live.id();
    let ok = send_text(ws, protocol::request(kind, &id, data)).await;
    if ok {
        live.pending.insert(id, (Instant::now(), p));
    } else if let Pending::Chapter { reply } = p {
        reply(ChapterOutcome::NotConnected);
    }
    ok
}

async fn request_status(ws: &mut Ws, live: &mut Live) -> bool {
    if live.status_outstanding {
        return true;
    }
    live.status_outstanding = true;
    send_request(
        ws,
        live,
        GET_RECORD_STATUS,
        None,
        Pending::Status {
            sent: Instant::now(),
        },
    )
    .await
}

async fn run_session(config: &ObsConfig, ctx: &mut Ctx) -> End {
    let opts = ctx.opts.clone();
    let (mut ws, hello) =
        match with_commands(handshake(config, &opts, EVENT_SUBSCRIPTIONS), config, ctx).await {
            Err(end) => return end,
            Ok(Err(fail)) => return fail.into_end(false),
            Ok(Ok(pair)) => pair,
        };
    let mut live = Live {
        pending: HashMap::new(),
        next_id: 0,
        announced: false,
        hello,
        recording: Recording::default(),
        seeded: false,
        chapters: ChapterSupport::Unknown,
        status_outstanding: false,
    };
    let end = session_loop(&mut ws, &mut live, config, ctx).await;
    live.fail_pending();
    if live.announced {
        ctx.emit(ObsEvent::Disconnected);
    }
    if matches!(end, End::Shutdown | End::Changed(_) | End::Reconnect) {
        close_politely(&mut ws).await;
    }
    end
}

async fn session_loop(ws: &mut Ws, live: &mut Live, config: &ObsConfig, ctx: &mut Ctx) -> End {
    let opts = ctx.opts.clone();
    if !send_request(ws, live, GET_VERSION, None, Pending::Version).await
        || !request_status(ws, live).await
    {
        return closed(None).into_end(true);
    }
    let mut poll = interval(
        opts.poll_interval
            .min(opts.request_timeout)
            .max(Duration::from_millis(10)),
    );
    poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if let Some(end) = on_message(t.as_str(), ws, live, ctx).await {
                        return end;
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    return closed(frame.map(|f| u16::from(f.code))).into_end(true);
                }
                Some(Ok(_)) => {}
                Some(Err(_)) => {
                    return End::Retry {
                        message: "The connection to OBS was lost.".to_string(),
                        identified: true,
                    };
                }
                None => return closed(None).into_end(true),
            },
            cmd = ctx.cmds.recv() => match cmd {
                None | Some(Command::Shutdown) => return End::Shutdown,
                Some(Command::Reconnect) => return End::Reconnect,
                Some(Command::SetConfig(c)) => {
                    if c != *config {
                        return End::Changed(c);
                    }
                }
                Some(Command::Chapter { name, reply }) => {
                    if !chapter(name, reply, ws, live).await {
                        return closed(None).into_end(true);
                    }
                }
            },
            _ = poll.tick() => {
                let now = Instant::now();
                let expired: Vec<String> = live
                    .pending
                    .iter()
                    .filter(|(_, (sent, _))| now.duration_since(*sent) >= opts.request_timeout)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in expired {
                    if let Some((_, p)) = live.pending.remove(&id) {
                        match p {
                            Pending::Chapter { reply } => reply(ChapterOutcome::Timeout),
                            Pending::Status { .. } => live.status_outstanding = false,
                            Pending::Version => {
                                return End::Retry {
                                    message: format!("OBS did not answer at {}.", config.display_addr()),
                                    identified: true,
                                };
                            }
                        }
                    }
                }
                if live.recording.state != RecordState::Stopped && !request_status(ws, live).await {
                    return closed(None).into_end(true);
                }
            }
        }
    }
}

/// Handles one server message. Returns `Some` when the session must end.
async fn on_message(text: &str, ws: &mut Ws, live: &mut Live, ctx: &Ctx) -> Option<End> {
    match protocol::parse(text)? {
        Incoming::Event { event_type, data } => match event_type.as_str() {
            "RecordStateChanged" => {
                let state = data
                    .get("outputState")
                    .and_then(Value::as_str)
                    .and_then(RecordState::from_output_state)?;
                let prev = live.recording.state;
                live.recording.state = state;
                live.seeded = true;
                match state {
                    RecordState::Stopped => {
                        live.recording.duration_ms = 0;
                        live.recording.timecode = crate::status::timecode(0);
                        live.recording.sample = None;
                    }
                    // A new recording may use another format.
                    RecordState::Starting | RecordState::Recording
                        if matches!(prev, RecordState::Stopped | RecordState::Starting)
                            && live.chapters == ChapterSupport::FormatUnsupported =>
                    {
                        live.chapters = ChapterSupport::Available;
                    }
                    _ => {}
                }
                live.publish(ctx);
                ctx.emit(ObsEvent::Record {
                    state,
                    at: Instant::now(),
                });
                if matches!(state, RecordState::Recording | RecordState::Paused)
                    && !request_status(ws, live).await
                {
                    return Some(closed(None).into_end(true));
                }
                None
            }
            "ExitStarted" => {
                ctx.emit(ObsEvent::Exiting);
                None
            }
            _ => None,
        },
        Incoming::Response {
            request_id,
            ok,
            code,
            data,
            ..
        } => {
            let (_, pending) = live.pending.remove(&request_id)?;
            match pending {
                Pending::Version => {
                    let v = Version::from_data(&data).filter(|_| ok);
                    let (obs_version, ws_version) = match &v {
                        Some(v) if !v.obs_version.is_empty() => {
                            (v.obs_version.clone(), v.ws_version.clone())
                        }
                        _ => (
                            live.hello.obs_version.clone(),
                            live.hello.ws_version.clone(),
                        ),
                    };
                    live.chapters = match &v {
                        Some(v) if v.has(CREATE_RECORD_CHAPTER) => ChapterSupport::Available,
                        Some(_) => ChapterSupport::NotInThisObs,
                        None => ChapterSupport::Unknown,
                    };
                    live.announced = true;
                    let rec = live.recording.clone();
                    let chapters = live.chapters;
                    let (o, w) = (obs_version.clone(), ws_version.clone());
                    ctx.set(|s| {
                        s.connection = Connection::Connected {
                            obs_version: o,
                            ws_version: w,
                        };
                        s.recording = Some(rec);
                        s.chapters = chapters;
                    });
                    tracing::info!(obs = %obs_version, ws = %ws_version, "connected to OBS");
                    ctx.emit(ObsEvent::Connected {
                        obs_version,
                        ws_version,
                    });
                    None
                }
                Pending::Status { sent } => {
                    live.status_outstanding = false;
                    let st = RecordStatus::from_data(&data).filter(|_| ok)?;
                    let now = Instant::now();
                    let at = sent + now.duration_since(sent) / 2;
                    if !live.seeded {
                        live.seeded = true;
                        live.recording.state = match (st.active, st.paused) {
                            (_, true) => RecordState::Paused,
                            (true, false) => RecordState::Recording,
                            (false, false) => RecordState::Stopped,
                        };
                    }
                    if live.recording.state != RecordState::Stopped {
                        live.recording.duration_ms = st.duration_ms;
                        live.recording.timecode = if st.timecode.is_empty() {
                            crate::status::timecode(st.duration_ms)
                        } else {
                            st.timecode.clone()
                        };
                        if st.active || st.paused {
                            let sample = DurationSample {
                                at,
                                duration_ms: st.duration_ms,
                                paused: st.paused,
                            };
                            live.recording.sample = Some(sample);
                            if live.announced {
                                ctx.emit(ObsEvent::Sample(sample));
                            }
                        }
                    }
                    live.publish(ctx);
                    None
                }
                Pending::Chapter { reply } => {
                    let outcome = if ok {
                        ChapterOutcome::Created
                    } else {
                        match code {
                            status_code::OUTPUT_NOT_RUNNING => ChapterOutcome::NotRecording,
                            status_code::OUTPUT_PAUSED => ChapterOutcome::Paused,
                            status_code::UNKNOWN_REQUEST_TYPE => {
                                live.chapters = ChapterSupport::NotInThisObs;
                                ChapterOutcome::NotInThisObs
                            }
                            status_code::REQUEST_PROCESSING_FAILED => {
                                if live.recording.state == RecordState::Paused {
                                    ChapterOutcome::Paused
                                } else {
                                    live.chapters = ChapterSupport::FormatUnsupported;
                                    ChapterOutcome::FormatUnsupported
                                }
                            }
                            other => ChapterOutcome::Failed { code: other },
                        }
                    };
                    live.publish(ctx);
                    reply(outcome);
                    None
                }
            }
        }
        Incoming::Hello { .. } | Incoming::Identified { .. } | Incoming::Other => None,
    }
}

/// Sends `CreateRecordChapter` when it can work; otherwise answers at once.
/// Returns false when the socket failed.
async fn chapter(name: Option<String>, reply: Reply, ws: &mut Ws, live: &mut Live) -> bool {
    if !live.announced {
        reply(ChapterOutcome::NotConnected);
        return true;
    }
    match live.chapters {
        ChapterSupport::NotInThisObs => {
            reply(ChapterOutcome::NotInThisObs);
            return true;
        }
        ChapterSupport::FormatUnsupported if live.recording.state == RecordState::Recording => {
            reply(ChapterOutcome::FormatUnsupported);
            return true;
        }
        _ => {}
    }
    match live.recording.state {
        RecordState::Recording => {}
        RecordState::Paused => {
            reply(ChapterOutcome::Paused);
            return true;
        }
        _ => {
            reply(ChapterOutcome::NotRecording);
            return true;
        }
    }
    let data = name.map(|n| json!({ "chapterName": n }));
    send_request(
        ws,
        live,
        CREATE_RECORD_CHAPTER,
        data,
        Pending::Chapter { reply },
    )
    .await
}

/// One-off connection test for the settings window (`obs_test`): connect,
/// identify (no event subscriptions), `GetVersion`, disconnect.
pub async fn test_connection(config: &ObsConfig, limit: Duration) -> ObsTestResult {
    let opts = ClientOptions {
        connect_timeout: limit,
        request_timeout: limit,
        ..ClientOptions::default()
    };
    let probe = async {
        let (mut ws, hello) = handshake(config, &opts, 0)
            .await
            .map_err(Fail::connection)?;
        let id = "ks-test";
        if !send_text(&mut ws, protocol::request(GET_VERSION, id, None)).await {
            return Err(closed(None).connection());
        }
        let version = loop {
            match read_text(&mut ws).await {
                Read::Text(t) => {
                    if let Some(Incoming::Response {
                        request_id, data, ..
                    }) = protocol::parse(&t)
                    {
                        if request_id == id {
                            break Version::from_data(&data);
                        }
                    }
                }
                Read::Closed(code) => return Err(closed(code).connection()),
                Read::Lost => return Err(closed(None).connection()),
            }
        };
        close_politely(&mut ws).await;
        Ok(match version {
            Some(v) if !v.obs_version.is_empty() => (v.obs_version, v.ws_version),
            _ => (hello.obs_version, hello.ws_version),
        })
    };
    match timeout(limit, probe).await {
        Ok(Ok((obs, ws))) => ObsTestResult {
            ok: true,
            obs_version: Some(obs),
            ws_version: Some(ws),
            error: None,
        },
        Ok(Err(conn)) => ObsTestResult {
            ok: false,
            error: conn
                .message()
                .or(Some("Could not connect to OBS.".to_string())),
            ..ObsTestResult::default()
        },
        Err(_) => ObsTestResult {
            ok: false,
            error: Some(format!("OBS did not answer at {}.", config.display_addr())),
            ..ObsTestResult::default()
        },
    }
}

/// [`test_connection`] for a plain thread (it runs its own small runtime;
/// do not call it from inside an async runtime).
pub fn test_connection_blocking(config: &ObsConfig, limit: Duration) -> ObsTestResult {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt.block_on(test_connection(config, limit)),
        Err(_) => ObsTestResult {
            ok: false,
            error: Some("Could not start the connection test.".to_string()),
            ..ObsTestResult::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let c = |h: &str, p: u16| ObsConfig::new(h, p, None).url();
        assert_eq!(c("127.0.0.1", 4455).unwrap(), "ws://127.0.0.1:4455");
        assert_eq!(c(" localhost ", 4455).unwrap(), "ws://localhost:4455");
        assert_eq!(
            c("studio-pc.local", 4460).unwrap(),
            "ws://studio-pc.local:4460"
        );
        assert_eq!(c("::1", 4455).unwrap(), "ws://[::1]:4455");
        assert_eq!(c("[::1]", 4455).unwrap(), "ws://[::1]:4455");
        for bad in ["", "a b", "http://x", "x/y", "a..b", "x:y", "x@y"] {
            assert!(c(bad, 4455).is_err(), "{bad}");
        }
        assert!(c("127.0.0.1", 0).is_err());
        assert_eq!(ObsConfig::default().url().unwrap(), "ws://127.0.0.1:4455");
    }

    #[test]
    fn empty_password_is_none_and_debug_hides_it() {
        assert!(ObsConfig::new("h", 1, Some(String::new()))
            .password
            .is_none());
        let c = ObsConfig::new("h", 1, Some("s3cret-pw".into()));
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("s3cret-pw"), "{dbg}");
        assert!(dbg.contains("<hidden>"));
    }

    #[test]
    fn backoff_doubles_with_jitter_and_caps() {
        let opts = ClientOptions::default();
        let mut j = Jitter(42);
        for (attempt, base) in [
            (0u32, 1.0f64),
            (1, 2.0),
            (2, 4.0),
            (3, 8.0),
            (4, 16.0),
            (5, 30.0),
            (40, 30.0),
        ] {
            let d = backoff(&opts, attempt, &mut j).as_secs_f64();
            assert!(
                d >= base * 0.8 - 1e-9 && d <= base * 1.2 + 1e-9,
                "{attempt}: {d}"
            );
        }
    }

    #[test]
    fn close_codes() {
        assert!(matches!(
            closed(Some(4009)),
            Fail::Hold(Connection::AuthFailed {
                missing_password: false
            })
        ));
        assert!(matches!(closed(Some(4011)), Fail::Hold(Connection::Kicked)));
        assert!(matches!(
            closed(Some(4010)),
            Fail::Hold(Connection::Unsupported { .. })
        ));
        assert!(matches!(closed(Some(4006)), Fail::Retry(ref m) if m.contains("4006")));
        assert!(matches!(closed(Some(1001)), Fail::Retry(_)));
        assert!(matches!(closed(None), Fail::Retry(_)));
    }

    #[test]
    fn same_target_ignores_case_and_spaces() {
        let a = ObsConfig::new("LocalHost", 4455, None);
        let b = ObsConfig::new(" localhost ", 4455, Some("x".into()));
        assert!(a.same_target(&b));
        assert!(!a.same_target(&ObsConfig::new("localhost", 4456, None)));
    }
}
