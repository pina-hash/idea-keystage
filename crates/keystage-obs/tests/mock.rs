//! Tests against a mock obs-websocket 5 server written here: authentication
//! success and failure, close codes, record events, status polling,
//! chapters, reconnect, and the one-off connection test.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use keystage_obs::{
    auth_string, test_connection, ChapterOutcome, ChapterSupport, ClientOptions, Connection,
    ObsClient, ObsConfig, ObsEvent, ObsStatus, RecordState,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

const SALT: &str = "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=";
const CHALLENGE: &str = "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=";

#[derive(Debug)]
struct MockState {
    password: Option<String>,
    rpc_version: u64,
    available: Vec<String>,
    /// `(active, paused, duration_ms)` as GetRecordStatus reports it.
    record: (bool, bool, u64),
    chapter_code: u32,
    identifies: Vec<Value>,
    requests: Vec<Value>,
    connections: usize,
}

enum MockCmd {
    Send(String),
    Close(u16),
    Drop,
}

struct Mock {
    port: u16,
    state: Arc<Mutex<MockState>>,
    conn: Arc<Mutex<Option<mpsc::UnboundedSender<MockCmd>>>>,
}

fn all_requests() -> Vec<String> {
    [
        "GetVersion",
        "GetRecordStatus",
        "StartRecord",
        "StopRecord",
        "CreateRecordChapter",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

impl Mock {
    async fn start(password: Option<&str>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(MockState {
            password: password.map(str::to_string),
            rpc_version: 1,
            available: all_requests(),
            record: (false, false, 0),
            chapter_code: 100,
            identifies: Vec::new(),
            requests: Vec::new(),
            connections: 0,
        }));
        let conn: Arc<Mutex<Option<mpsc::UnboundedSender<MockCmd>>>> = Arc::new(Mutex::new(None));
        let (st, cn) = (state.clone(), conn.clone());
        tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let (st, cn) = (st.clone(), cn.clone());
                tokio::spawn(async move { serve_one(tcp, st, cn).await });
            }
        });
        Mock { port, state, conn }
    }

    fn config(&self, password: Option<&str>) -> ObsConfig {
        ObsConfig::new("127.0.0.1", self.port, password.map(str::to_string))
    }

    fn cmd(&self, c: MockCmd) {
        if let Some(tx) = self.conn.lock().unwrap().as_ref() {
            let _ = tx.send(c);
        }
    }

    fn event(&self, event_type: &str, data: Value) {
        let msg = json!({"op":5,"d":{"eventType":event_type,"eventIntent":64,"eventData":data}});
        self.cmd(MockCmd::Send(msg.to_string()));
    }

    fn record_event(&self, state: &str, active: bool) {
        self.event(
            "RecordStateChanged",
            json!({"outputActive":active,"outputState":state,"outputPath":null}),
        );
    }

    fn connections(&self) -> usize {
        self.state.lock().unwrap().connections
    }

    fn requests_of(&self, kind: &str) -> Vec<Value> {
        self.state
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|r| r["requestType"] == kind)
            .cloned()
            .collect()
    }
}

fn timecode(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms % 3_600_000 / 60_000,
        ms % 60_000 / 1000,
        ms % 1000
    )
}

async fn serve_one(
    tcp: tokio::net::TcpStream,
    st: Arc<Mutex<MockState>>,
    cn: Arc<Mutex<Option<mpsc::UnboundedSender<MockCmd>>>>,
) {
    let Ok(mut ws) = tokio_tungstenite::accept_async(tcp).await else {
        return;
    };
    let (password, rpc) = {
        let mut s = st.lock().unwrap();
        s.connections += 1;
        (s.password.clone(), s.rpc_version)
    };
    let mut d = json!({"obsStudioVersion":"32.2.2","obsWebSocketVersion":"5.7.4","rpcVersion":rpc});
    if password.is_some() {
        d["authentication"] = json!({"challenge": CHALLENGE, "salt": SALT});
    }
    if ws
        .send(Message::text(json!({"op":0,"d":d}).to_string()))
        .await
        .is_err()
    {
        return;
    }
    let identify: Value = loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => break serde_json::from_str(t.as_str()).unwrap(),
            Some(Ok(_)) => continue,
            _ => return,
        }
    };
    st.lock().unwrap().identifies.push(identify["d"].clone());
    if let Some(pw) = &password {
        let expected = auth_string(pw, SALT, CHALLENGE);
        if identify["d"]["authentication"] != json!(expected) {
            let _ = ws
                .close(Some(CloseFrame {
                    code: CloseCode::from(4009),
                    reason: "Authentication failed.".into(),
                }))
                .await;
            return;
        }
    }
    if identify["d"]["rpcVersion"] != json!(1) {
        let _ = ws
            .close(Some(CloseFrame {
                code: CloseCode::from(4010),
                reason: "".into(),
            }))
            .await;
        return;
    }
    if ws
        .send(Message::text(r#"{"op":2,"d":{"negotiatedRpcVersion":1}}"#))
        .await
        .is_err()
    {
        return;
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    *cn.lock().unwrap() = Some(tx);
    loop {
        tokio::select! {
            msg = ws.next() => {
                let Some(Ok(Message::Text(t))) = msg else {
                    match msg {
                        Some(Ok(_)) => continue,
                        _ => return,
                    }
                };
                let v: Value = serde_json::from_str(t.as_str()).unwrap();
                let d = v["d"].clone();
                st.lock().unwrap().requests.push(d.clone());
                let kind = d["requestType"].as_str().unwrap_or("").to_string();
                let id = d["requestId"].clone();
                let (status, data) = {
                    let s = st.lock().unwrap();
                    match kind.as_str() {
                        "GetVersion" => (
                            json!({"result":true,"code":100}),
                            json!({"obsVersion":"32.2.2","obsWebSocketVersion":"5.7.4","rpcVersion":1,
                                   "availableRequests":s.available,"supportedImageFormats":["png"],
                                   "platform":"windows","platformDescription":"Windows 11 (10.0)"}),
                        ),
                        "GetRecordStatus" => {
                            let (active, paused, dur) = s.record;
                            (
                                json!({"result":true,"code":100}),
                                json!({"outputActive":active,"outputPaused":paused,
                                       "outputTimecode":timecode(dur),"outputDuration":dur,"outputBytes":0}),
                            )
                        }
                        "CreateRecordChapter" => {
                            let code = s.chapter_code;
                            (json!({"result":code == 100,"code":code}), Value::Null)
                        }
                        _ => (json!({"result":false,"code":204}), Value::Null),
                    }
                };
                let mut resp = json!({"op":7,"d":{"requestType":kind,"requestId":id,"requestStatus":status}});
                if !data.is_null() {
                    resp["d"]["responseData"] = data;
                }
                if ws.send(Message::text(resp.to_string())).await.is_err() {
                    return;
                }
            }
            cmd = rx.recv() => match cmd {
                Some(MockCmd::Send(s)) => {
                    if ws.send(Message::text(s)).await.is_err() {
                        return;
                    }
                }
                Some(MockCmd::Close(code)) => {
                    let _ = ws
                        .close(Some(CloseFrame { code: CloseCode::from(code), reason: "".into() }))
                        .await;
                    return;
                }
                Some(MockCmd::Drop) | None => return,
            },
        }
    }
}

fn fast() -> ClientOptions {
    ClientOptions {
        connect_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(1),
        poll_interval: Duration::from_millis(50),
        backoff_initial: Duration::from_millis(20),
        backoff_max: Duration::from_millis(100),
    }
}

async fn wait_status(
    client: &ObsClient,
    what: &str,
    pred: impl Fn(&ObsStatus) -> bool,
) -> ObsStatus {
    let mut w = client.watch();
    let res = tokio::time::timeout(Duration::from_secs(5), w.wait_for(|s| pred(s)))
        .await
        .map(|r| r.map(|s| s.clone()));
    match res {
        Ok(Ok(s)) => s,
        _ => panic!(
            "timed out waiting for {what}; status is {:?}",
            client.status()
        ),
    }
}

fn connected(s: &ObsStatus) -> bool {
    s.connection.is_connected()
}

fn rec_state(s: &ObsStatus) -> Option<RecordState> {
    s.recording.as_ref().map(|r| r.state)
}

/// Waits until the mock has read at least `n` requests of `kind`. The client
/// can send a request before the status a test waits on changes, while the
/// mock reads one frame at a time (it answers `GetVersion` before it reads the
/// `GetRecordStatus` sent right after it), so a request the client already
/// sent may not be recorded yet when that status arrives.
async fn wait_requests(mock: &Mock, kind: &str, n: usize) -> Vec<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let reqs = mock.requests_of(kind);
        if reqs.len() >= n {
            return reqs;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {n} {kind} request(s); the mock read {}",
            reqs.len()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Lets the client run with a mock that should not hear from it again.
async fn quiet(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticates_and_identifies_with_rpc_1_and_subscriptions_65() {
    let mock = Mock::start(Some("supersecretpassword")).await;
    let client = ObsClient::spawn_with(mock.config(Some("supersecretpassword")), fast()).unwrap();
    let mut events = client.events();
    let s = wait_status(&client, "connected", connected).await;
    assert_eq!(
        s.connection,
        Connection::Connected {
            obs_version: "32.2.2".into(),
            ws_version: "5.7.4".into()
        }
    );
    assert_eq!(s.chapters, ChapterSupport::Available);
    assert_eq!(rec_state(&s), Some(RecordState::Stopped));
    let id = mock.state.lock().unwrap().identifies[0].clone();
    assert_eq!(id["rpcVersion"], 1);
    assert_eq!(id["eventSubscriptions"], 65);
    // The B.4 vector: the mock uses the protocol's example salt and challenge.
    assert_eq!(
        id["authentication"],
        "1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4="
    );
    // GetVersion and GetRecordStatus right after Identified.
    assert_eq!(mock.requests_of("GetVersion").len(), 1);
    wait_requests(&mock, "GetRecordStatus", 1).await;
    let first = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        first,
        ObsEvent::Connected {
            obs_version: "32.2.2".into(),
            ws_version: "5.7.4".into()
        }
    );
    let ipc = s.to_ipc(true);
    assert_eq!(ipc.recording.unwrap().timecode, "00:00:00.000");
    client.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_password_when_obs_asks_for_none() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(Some("unused")), fast()).unwrap();
    wait_status(&client, "connected", connected).await;
    let id = mock.state.lock().unwrap().identifies[0].clone();
    assert!(id.get("authentication").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_password_stops_until_the_password_changes() {
    let mock = Mock::start(Some("right-password")).await;
    let client = ObsClient::spawn_with(mock.config(Some("wrong-password")), fast()).unwrap();
    let s = wait_status(&client, "auth failed", |s| {
        matches!(s.connection, Connection::AuthFailed { .. })
    })
    .await;
    assert_eq!(
        s.connection,
        Connection::AuthFailed {
            missing_password: false
        }
    );
    assert!(s.recording.is_none());
    quiet(400).await;
    assert_eq!(mock.connections(), 1, "no retry after close code 4009");
    // The same password again: still no retry.
    client.set_config(mock.config(Some("wrong-password")));
    quiet(200).await;
    assert_eq!(mock.connections(), 1);
    // A different password retries.
    client.set_config(mock.config(Some("right-password")));
    wait_status(&client, "connected", connected).await;
    assert_eq!(mock.connections(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_password_is_reported_without_identifying() {
    let mock = Mock::start(Some("pw-required")).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let s = wait_status(&client, "auth failed", |s| {
        matches!(s.connection, Connection::AuthFailed { .. })
    })
    .await;
    assert_eq!(
        s.connection,
        Connection::AuthFailed {
            missing_password: true
        }
    );
    quiet(200).await;
    assert!(mock.state.lock().unwrap().identifies.is_empty());
    assert!(s
        .to_ipc(true)
        .error
        .unwrap()
        .contains("asks for a password"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kicked_does_not_reconnect_until_asked() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let mut events = client.events();
    wait_status(&client, "connected", connected).await;
    mock.cmd(MockCmd::Close(4011));
    wait_status(&client, "kicked", |s| s.connection == Connection::Kicked).await;
    quiet(400).await;
    assert_eq!(
        mock.connections(),
        1,
        "4011: must not reconnect automatically"
    );
    // Connected, then Disconnected.
    let mut seen = Vec::new();
    while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(100), events.recv()).await {
        seen.push(e);
    }
    assert!(seen.contains(&ObsEvent::Disconnected), "{seen:?}");
    client.reconnect();
    wait_status(&client, "connected again", connected).await;
    assert_eq!(mock.connections(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_rpc_version_is_reported() {
    let mock = Mock::start(None).await;
    mock.state.lock().unwrap().rpc_version = 0;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let s = wait_status(&client, "unsupported", |s| {
        matches!(s.connection, Connection::Unsupported { .. })
    })
    .await;
    assert!(s.connection.message().unwrap().contains("OBS 28"));
    quiet(300).await;
    assert_eq!(mock.connections(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_code_4010_is_unsupported() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    wait_status(&client, "connected", connected).await;
    mock.cmd(MockCmd::Close(4010));
    wait_status(&client, "unsupported", |s| {
        matches!(s.connection, Connection::Unsupported { .. })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_state_follows_output_state_not_output_active() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let mut events = client.events();
    wait_status(&client, "connected", connected).await;

    mock.state.lock().unwrap().record = (false, false, 0);
    mock.record_event("OBS_WEBSOCKET_OUTPUT_STARTING", false);
    wait_status(&client, "starting", |s| {
        rec_state(s) == Some(RecordState::Starting)
    })
    .await;
    mock.state.lock().unwrap().record = (true, false, 1_000);
    mock.record_event("OBS_WEBSOCKET_OUTPUT_STARTED", true);
    let s = wait_status(&client, "recording", |s| {
        rec_state(s) == Some(RecordState::Recording)
    })
    .await;
    assert!(s.recording.as_ref().unwrap().active());
    // outputActive is false while paused; the state still says paused.
    mock.state.lock().unwrap().record = (true, true, 1_500);
    mock.record_event("OBS_WEBSOCKET_OUTPUT_PAUSED", false);
    let s = wait_status(&client, "paused", |s| {
        rec_state(s) == Some(RecordState::Paused)
    })
    .await;
    let r = s.recording.unwrap();
    assert!(r.active() && r.paused());
    assert!(s.connection.is_connected());
    let wire = client.status().to_ipc(true).recording.unwrap();
    assert!(wire.active && wire.paused);
    mock.state.lock().unwrap().record = (true, false, 1_600);
    mock.record_event("OBS_WEBSOCKET_OUTPUT_RESUMED", true);
    wait_status(&client, "resumed", |s| {
        rec_state(s) == Some(RecordState::Recording)
    })
    .await;
    mock.record_event("OBS_WEBSOCKET_OUTPUT_STOPPING", false);
    wait_status(&client, "stopping", |s| {
        rec_state(s) == Some(RecordState::Stopping)
    })
    .await;
    mock.state.lock().unwrap().record = (false, false, 0);
    mock.record_event("OBS_WEBSOCKET_OUTPUT_STOPPED", false);
    let s = wait_status(&client, "stopped", |s| {
        rec_state(s) == Some(RecordState::Stopped)
    })
    .await;
    assert_eq!(s.recording.unwrap().duration_ms, 0);

    // The record events arrive in order on the broadcast channel.
    let mut states = Vec::new();
    while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(200), events.recv()).await {
        if let ObsEvent::Record { state, .. } = e {
            states.push(state);
        }
    }
    assert_eq!(
        states,
        [
            RecordState::Starting,
            RecordState::Recording,
            RecordState::Paused,
            RecordState::Recording,
            RecordState::Stopping,
            RecordState::Stopped
        ]
    );
    // Unknown states and other events are ignored.
    mock.record_event("OBS_WEBSOCKET_OUTPUT_RECONNECTING", false);
    mock.event("ExitStarted", Value::Null);
    let e = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e, ObsEvent::Exiting);
    assert_eq!(rec_state(&client.status()), Some(RecordState::Stopped));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn seeds_state_from_get_record_status_and_polls_while_recording() {
    let mock = Mock::start(None).await;
    // OBS is already recording when Keystage connects.
    mock.state.lock().unwrap().record = (true, false, 60_000);
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let mut events = client.events();
    let s = wait_status(&client, "seeded recording", |s| {
        rec_state(s) == Some(RecordState::Recording)
    })
    .await;
    assert_eq!(s.recording.as_ref().unwrap().duration_ms, 60_000);
    assert_eq!(s.recording.unwrap().timecode, "00:01:00.000");
    // Polling picks up the advancing duration.
    mock.state.lock().unwrap().record = (true, false, 61_250);
    let s = wait_status(&client, "polled duration", |s| {
        s.recording
            .as_ref()
            .is_some_and(|r| r.duration_ms == 61_250)
    })
    .await;
    let r = s.recording.unwrap();
    assert_eq!(r.timecode, "00:01:01.250");
    assert_eq!(r.sample.unwrap().duration_ms, 61_250);
    let polls_before = mock.requests_of("GetRecordStatus").len();
    quiet(300).await;
    let polls_after = mock.requests_of("GetRecordStatus").len();
    assert!(
        polls_after >= polls_before + 3,
        "{polls_before} then {polls_after}"
    );
    // Samples keep coming while recording, so read for a fixed time.
    let mut samples = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
    while let Ok(Ok(e)) = tokio::time::timeout_at(deadline, events.recv()).await {
        if let ObsEvent::Sample(s) = e {
            assert!(s.duration_ms >= 60_000 && !s.paused);
            samples += 1;
        }
    }
    assert!(samples >= 2);
    // Seeded paused.
    let mock2 = Mock::start(None).await;
    mock2.state.lock().unwrap().record = (true, true, 5_000);
    let c2 = ObsClient::spawn_with(mock2.config(None), fast()).unwrap();
    wait_status(&c2, "seeded paused", |s| {
        rec_state(s) == Some(RecordState::Paused)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_polling_while_stopped() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    wait_status(&client, "connected", connected).await;
    // The seed request goes out right after Identified; count from it, or
    // its late arrival at the mock would look like a poll.
    let n = wait_requests(&mock, "GetRecordStatus", 1).await.len();
    quiet(300).await;
    assert_eq!(mock.requests_of("GetRecordStatus").len(), n);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnects_after_the_connection_drops() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let mut events = client.events();
    wait_status(&client, "connected", connected).await;
    mock.cmd(MockCmd::Drop);
    wait_status(&client, "reconnected", |s| {
        connected(s) && mock.connections() == 2
    })
    .await;
    mock.cmd(MockCmd::Close(1001));
    wait_status(&client, "reconnected again", |s| {
        connected(s) && mock.connections() == 3
    })
    .await;
    let mut seq = Vec::new();
    while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(100), events.recv()).await {
        seq.push(matches!(e, ObsEvent::Connected { .. }));
    }
    // Connected, Disconnected, Connected, Disconnected, Connected.
    assert_eq!(seq, [true, false, true, false, true]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retries_with_backoff_until_obs_appears() {
    // Reserve a port, then free it: nothing listens there yet.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let client = ObsClient::spawn_with(ObsConfig::new("127.0.0.1", port, None), fast()).unwrap();
    let s = wait_status(&client, "error", |s| {
        matches!(s.connection, Connection::Error(_))
    })
    .await;
    let msg = s.connection.message().unwrap();
    assert!(msg.contains(&format!("127.0.0.1:{port}")), "{msg}");
    assert!(msg.contains("Enable WebSocket server"), "{msg}");
    // Moving to a running mock reconnects at once.
    let mock = Mock::start(None).await;
    client.set_config(mock.config(None));
    wait_status(&client, "connected", connected).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_password_while_connected_reconnects() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(Some("a")), fast()).unwrap();
    wait_status(&client, "connected", connected).await;
    client.set_config(mock.config(Some("a")));
    quiet(200).await;
    assert_eq!(mock.connections(), 1, "same settings: no reconnect");
    client.set_config(mock.config(Some("b")));
    wait_status(&client, "reconnected", |s| {
        connected(s) && mock.connections() == 2
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapters_are_best_effort_and_reported_cleanly() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    wait_status(&client, "connected", connected).await;
    let wait = Duration::from_secs(3);

    // Not recording: answered without asking OBS.
    assert_eq!(
        client.create_chapter(Some("Sketch")).await,
        ChapterOutcome::NotRecording
    );
    assert!(mock.requests_of("CreateRecordChapter").is_empty());

    mock.state.lock().unwrap().record = (true, false, 1_000);
    mock.record_event("OBS_WEBSOCKET_OUTPUT_STARTED", true);
    wait_status(&client, "recording", |s| {
        rec_state(s) == Some(RecordState::Recording)
    })
    .await;

    // Hybrid MP4: created, with the name passed through.
    assert_eq!(
        client.create_chapter(Some("  Sketch \n the base ")).await,
        ChapterOutcome::Created
    );
    let reqs = mock.requests_of("CreateRecordChapter");
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["requestData"]["chapterName"], "Sketch the base");
    // No name: OBS names it.
    assert_eq!(client.create_chapter(None).await, ChapterOutcome::Created);
    assert!(mock.requests_of("CreateRecordChapter")[1]
        .get("requestData")
        .is_none());

    // MKV: 702 means the format has no chapters.
    mock.state.lock().unwrap().chapter_code = 702;
    let o = client.create_chapter(Some("Extrude")).await;
    assert_eq!(o, ChapterOutcome::FormatUnsupported);
    assert!(o.message().contains("use Hybrid MP4"));
    wait_status(&client, "format unsupported", |s| {
        s.chapters == ChapterSupport::FormatUnsupported
    })
    .await;
    // Not asked again during this recording.
    let n = mock.requests_of("CreateRecordChapter").len();
    assert_eq!(
        client.create_chapter(Some("Again")).await,
        ChapterOutcome::FormatUnsupported
    );
    assert_eq!(mock.requests_of("CreateRecordChapter").len(), n);

    // Paused: answered without asking OBS.
    mock.record_event("OBS_WEBSOCKET_OUTPUT_PAUSED", false);
    wait_status(&client, "paused", |s| {
        rec_state(s) == Some(RecordState::Paused)
    })
    .await;
    assert_eq!(
        client.create_chapter(Some("x")).await,
        ChapterOutcome::Paused
    );

    // A new recording resets the format check.
    mock.record_event("OBS_WEBSOCKET_OUTPUT_STOPPED", false);
    wait_status(&client, "stopped", |s| {
        rec_state(s) == Some(RecordState::Stopped)
    })
    .await;
    mock.state.lock().unwrap().chapter_code = 100;
    mock.record_event("OBS_WEBSOCKET_OUTPUT_STARTED", true);
    wait_status(&client, "available again", |s| {
        rec_state(s) == Some(RecordState::Recording) && s.chapters == ChapterSupport::Available
    })
    .await;
    // The fire-and-forget form reports through events.
    let mut events = client.events();
    client.add_chapter(Some("Wrap up"));
    loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv()).await {
            Ok(Ok(ObsEvent::Chapter(o))) => {
                assert_eq!(o, ChapterOutcome::Created);
                break;
            }
            Ok(Ok(_)) => continue,
            other => panic!("no chapter event: {other:?}"),
        }
    }
    // Other failures carry their code.
    mock.state.lock().unwrap().chapter_code = 205;
    let blocking = {
        let c = &client;
        tokio::task::block_in_place(|| c.create_chapter_blocking(Some("y"), wait))
    };
    assert_eq!(blocking, ChapterOutcome::Failed { code: 205 });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapters_need_a_new_enough_obs() {
    let mock = Mock::start(None).await;
    {
        let mut s = mock.state.lock().unwrap();
        s.available = vec!["GetVersion".into(), "GetRecordStatus".into()];
        s.record = (true, false, 1_000);
    }
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let s = wait_status(&client, "connected", connected).await;
    assert_eq!(s.chapters, ChapterSupport::NotInThisObs);
    let o = client.create_chapter(Some("x")).await;
    assert_eq!(o, ChapterOutcome::NotInThisObs);
    assert!(o.message().contains("30.2"));
    assert!(mock.requests_of("CreateRecordChapter").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapters_while_disconnected_say_so() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let client = ObsClient::spawn_with(ObsConfig::new("127.0.0.1", port, None), fast()).unwrap();
    assert_eq!(
        client.create_chapter(Some("x")).await,
        ChapterOutcome::NotConnected
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_reports_off() {
    let mock = Mock::start(None).await;
    let client = ObsClient::spawn_with(mock.config(None), fast()).unwrap();
    let w = client.watch();
    wait_status(&client, "connected", connected).await;
    tokio::task::block_in_place(|| client.shutdown());
    assert_eq!(w.borrow().connection, Connection::Off);
    assert!(w.borrow().recording.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_test_for_the_settings_window() {
    let mock = Mock::start(Some("pw-123456")).await;
    let ok = test_connection(&mock.config(Some("pw-123456")), Duration::from_secs(3)).await;
    assert!(ok.ok, "{ok:?}");
    assert_eq!(ok.obs_version.as_deref(), Some("32.2.2"));
    assert_eq!(ok.ws_version.as_deref(), Some("5.7.4"));
    let id = mock.state.lock().unwrap().identifies[0].clone();
    assert_eq!(id["eventSubscriptions"], 0, "a test needs no events");
    let bad = test_connection(&mock.config(Some("nope")), Duration::from_secs(3)).await;
    assert!(!bad.ok);
    assert!(bad.error.unwrap().contains("did not accept the password"));
    let missing = test_connection(&mock.config(None), Duration::from_secs(3)).await;
    assert!(missing.error.unwrap().contains("asks for a password"));
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let refused = test_connection(
        &ObsConfig::new("127.0.0.1", port, None),
        Duration::from_secs(3),
    )
    .await;
    assert!(!refused.ok);
    assert!(refused.error.unwrap().contains("Could not reach OBS"));
    let invalid = test_connection(
        &ObsConfig::new("not a host", 4455, None),
        Duration::from_secs(3),
    )
    .await;
    assert!(invalid.error.unwrap().contains("not valid"));
    // The JSON shape the settings window reads.
    let v = serde_json::to_value(&ok).unwrap();
    assert_eq!(
        v,
        json!({"ok":true,"obs_version":"32.2.2","ws_version":"5.7.4"})
    );
}

#[test]
fn blocking_connection_test_runs_its_own_runtime() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let r = keystage_obs::test_connection_blocking(
        &ObsConfig::new("127.0.0.1", port, None),
        Duration::from_secs(2),
    );
    assert!(!r.ok && r.error.is_some());
}
