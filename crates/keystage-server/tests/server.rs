//! End-to-end tests over real sockets: routes, Host and Origin rules,
//! hello-first, ordering, lag recovery, client counts, binding and shutdown.

use std::net::{Ipv4Addr, SocketAddr, TcpStream as StdTcpStream, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use keystage_core::protocol::{ClientHost, Route, ServerMsg};
use keystage_server::{Hub, MemoryAssets, Server, ServerConfig, ServerError, ServerHandle};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const OVERLAY_HTML: &str = "<!doctype html><title>overlay fixture</title>";
const HELLO: &str = r#"{"t":"hello","fixture":true}"#;

fn assets() -> Arc<MemoryAssets> {
    let mut m = MemoryAssets::new();
    m.insert("overlay.html", OVERLAY_HTML)
        .insert("assets/app-abc123.js", "console.log(1)")
        .insert("assets/app-abc123.css", "body{}")
        .insert("fonts/x.woff2", vec![0x77u8, 0x4f, 0x46, 0x32])
        .insert("sounds/click.wav", vec![0x52u8, 0x49, 0x46, 0x46]);
    Arc::new(m)
}

fn start_with(hub: Hub, config: ServerConfig) -> ServerHandle {
    Server::start(config, assets(), hub).expect("server starts")
}

fn start(hub: Hub) -> ServerHandle {
    start_with(hub, ServerConfig::new(0))
}

fn hub() -> Hub {
    let hub = Hub::new();
    hub.set_snapshot(|| HELLO.to_string());
    hub
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A minimal HTTP/1.1 client: one request, `Connection: close`.
async fn http(port: u16, method: &str, path: &str, extra: &[(&str, &str)]) -> HttpResponse {
    let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !extra.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    for (k, v) in extra {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut raw))
        .await
        .expect("response in time")
        .unwrap();
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("header end");
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    HttpResponse {
        status,
        headers,
        body: raw[split + 4..].to_vec(),
    }
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn ws_connect(port: u16, host: Option<&str>, origin: Option<&str>) -> Result<Ws, WsError> {
    let mut req = format!("ws://127.0.0.1:{port}/ws")
        .into_client_request()
        .unwrap();
    if let Some(h) = host {
        req.headers_mut()
            .insert("Host", HeaderValue::from_str(h).unwrap());
    }
    if let Some(o) = origin {
        req.headers_mut()
            .insert("Origin", HeaderValue::from_str(o).unwrap());
    }
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    tokio_tungstenite::client_async(req, MaybeTlsStream::Plain(stream))
        .await
        .map(|(ws, _)| ws)
}

async fn next_text(ws: &mut Ws) -> String {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("a message in time")
            .expect("stream open")
            .expect("no error");
        match msg {
            Message::Text(t) => return t.as_str().to_string(),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected {other:?}"),
        }
    }
}

async fn wait_counts(hub: &Hub, pred: impl Fn(&keystage_server::ClientCounts) -> bool) {
    let mut w = hub.watch_counts();
    tokio::time::timeout(Duration::from_secs(5), w.wait_for(|c| pred(c)))
        .await
        .expect("counts in time")
        .unwrap();
}

#[tokio::test]
async fn health() {
    let server = start(hub());
    let r = http(server.port(), "GET", "/api/health", &[]).await;
    assert_eq!(r.status, 200);
    assert_eq!(
        String::from_utf8(r.body.clone()).unwrap(),
        r#"{"ok":true,"app":"IDEA Keystage","version":"0.1.0","protocol":1}"#
    );
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(r.header("cache-control"), Some("no-store"));
}

#[tokio::test]
async fn overlay_routes_serve_overlay_html() {
    let server = start(hub());
    let port = server.port();
    for route in ["keys", "keyboard", "mouse", "cursor", "all"] {
        for q in ["", "?theme=carbon&scale=2"] {
            let r = http(port, "GET", &format!("/overlay/{route}{q}"), &[]).await;
            assert_eq!(r.status, 200, "{route}");
            assert_eq!(String::from_utf8(r.body.clone()).unwrap(), OVERLAY_HTML);
            assert_eq!(r.header("content-type"), Some("text/html; charset=utf-8"));
            assert_eq!(r.header("cache-control"), Some("no-store"));
            let csp = r.header("content-security-policy").unwrap();
            assert!(csp.contains(&format!("ws://127.0.0.1:{port}")), "{csp}");
            assert!(r
                .headers
                .iter()
                .all(|(k, _)| !k.to_ascii_lowercase().starts_with("access-control-")));
        }
    }
    // HEAD works like GET without a body.
    let r = http(port, "HEAD", "/overlay/keys", &[]).await;
    assert_eq!(r.status, 200);
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn static_files_have_correct_types() {
    let server = start(hub());
    let port = server.port();
    let cases = [
        (
            "/assets/app-abc123.js",
            "text/javascript; charset=utf-8",
            "public, max-age=31536000, immutable",
        ),
        (
            "/assets/app-abc123.css",
            "text/css; charset=utf-8",
            "public, max-age=31536000, immutable",
        ),
        ("/fonts/x.woff2", "font/woff2", "no-cache"),
        ("/sounds/click.wav", "audio/wav", "no-cache"),
    ];
    for (path, ctype, cache) in cases {
        let r = http(port, "GET", path, &[]).await;
        assert_eq!(r.status, 200, "{path}");
        assert_eq!(r.header("content-type"), Some(ctype), "{path}");
        assert_eq!(r.header("cache-control"), Some(cache), "{path}");
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
        assert!(r.header("access-control-allow-origin").is_none());
    }
}

#[tokio::test]
async fn everything_else_is_404() {
    let server = start(hub());
    let port = server.port();
    for path in [
        "/",
        "/overlay",
        "/overlay/",
        "/overlay/nope",
        "/overlay/keys/extra",
        "/overlay.html",
        "/settings.html",
        "/assets/missing.js",
        "/assets/../overlay.html",
        "/assets/%2e%2e/overlay.html",
        "/assets/.hidden",
        "/api",
        "/api/health/x",
        "/favicon.ico",
    ] {
        let r = http(port, "GET", path, &[]).await;
        assert_eq!(r.status, 404, "{path}");
        assert!(r.header("access-control-allow-origin").is_none());
    }
    for (method, path) in [
        ("POST", "/api/health"),
        ("PUT", "/overlay/keys"),
        ("DELETE", "/assets/app-abc123.js"),
        ("POST", "/ws"),
        ("OPTIONS", "/api/health"),
    ] {
        let r = http(port, method, path, &[]).await;
        assert_eq!(r.status, 404, "{method} {path}");
    }
}

#[tokio::test]
async fn ws_rejects_bad_origin_and_bad_host() {
    let server = start(hub());
    let port = server.port();
    let cases = [
        (None, Some("http://evil.example")),
        (None, Some("null")),
        (None, Some("http://absolute")),
        (None, Some(&*format!("https://127.0.0.1:{port}"))),
        (
            None,
            Some(&*format!("http://127.0.0.1:{}", port.wrapping_add(1))),
        ),
        (Some(&*format!("attacker.example:{port}")), None),
        (
            Some(&*format!("attacker.example:{port}")),
            Some(&*format!("http://attacker.example:{port}")),
        ),
        (Some("127.0.0.1"), None),
    ];
    for (host, origin) in cases {
        match ws_connect(port, host, origin).await {
            Err(WsError::Http(resp)) => assert_eq!(resp.status(), 403, "{host:?} {origin:?}"),
            Err(e) => panic!("{host:?} {origin:?}: unexpected {e}"),
            Ok(_) => panic!("{host:?} {origin:?} was accepted"),
        }
    }
    // A plain (non-upgrade) request with a bad Origin is refused too.
    let r = http(port, "GET", "/ws", &[("Origin", "http://evil.example")]).await;
    assert_eq!(r.status, 403);
    // A plain request with good headers is not a WebSocket.
    let r = http(port, "GET", "/ws", &[]).await;
    assert!(r.status >= 400 && r.status != 403, "{}", r.status);
}

#[tokio::test]
async fn ws_accepts_loopback_origins_and_sends_hello_first() {
    let hub = hub();
    let server = start(hub.clone());
    let port = server.port();
    let host_lh = format!("localhost:{port}");
    let o1 = format!("http://127.0.0.1:{port}");
    let o2 = format!("http://localhost:{port}");
    let cases: [(Option<&str>, Option<&str>); 4] = [
        (None, None),
        (None, Some(&o1)),
        (Some(&host_lh), Some(&o2)),
        (Some(&host_lh), None),
    ];
    for (host, origin) in cases {
        let mut ws = ws_connect(port, host, origin)
            .await
            .unwrap_or_else(|e| panic!("{host:?} {origin:?}: {e}"));
        assert_eq!(next_text(&mut ws).await, HELLO);
        ws.close(None).await.unwrap();
    }
}

#[tokio::test]
async fn hello_is_current_and_ping_is_answered() {
    let hub = Hub::new();
    let server = start(hub.clone());
    let mut ws = ws_connect(server.port(), None, None).await.unwrap();
    // The default snapshot is a real protocol hello.
    let first = next_text(&mut ws).await;
    assert!(matches!(
        ServerMsg::from_json(&first).unwrap(),
        ServerMsg::Hello { .. }
    ));
    ws.send(Message::text(r#"{"t":"ping"}"#)).await.unwrap();
    assert_eq!(next_text(&mut ws).await, r#"{"t":"pong"}"#);
    // Garbage and binary frames are ignored, the connection stays open.
    ws.send(Message::text("not json")).await.unwrap();
    ws.send(Message::binary(vec![1u8, 2, 3])).await.unwrap();
    ws.send(Message::text(r#"{"t":"ping"}"#)).await.unwrap();
    assert_eq!(next_text(&mut ws).await, r#"{"t":"pong"}"#);
    // A new snapshot provider is used for the next client.
    hub.set_snapshot(|| r#"{"t":"hello","n":2}"#.to_string());
    let mut ws2 = ws_connect(server.port(), None, None).await.unwrap();
    assert_eq!(next_text(&mut ws2).await, r#"{"t":"hello","n":2}"#);
}

#[tokio::test]
async fn broadcast_is_delivered_in_order_to_every_client() {
    let hub = hub();
    let server = start(hub.clone());
    let mut a = ws_connect(server.port(), None, None).await.unwrap();
    let mut b = ws_connect(server.port(), None, None).await.unwrap();
    assert_eq!(next_text(&mut a).await, HELLO);
    assert_eq!(next_text(&mut b).await, HELLO);
    wait_counts(&hub, |c| c.total == 2).await;
    for i in 0..500 {
        hub.publish(format!(r#"{{"t":"test","seq":{i}}}"#));
    }
    for ws in [&mut a, &mut b] {
        for i in 0..500 {
            assert_eq!(next_text(ws).await, format!(r#"{{"t":"test","seq":{i}}}"#));
        }
    }
}

#[tokio::test]
async fn a_lagging_client_recovers_with_a_fresh_hello() {
    let hub = Hub::with_capacity(16);
    hub.set_snapshot(|| HELLO.to_string());
    let server = start(hub.clone());
    let mut ws = ws_connect(server.port(), None, None).await.unwrap();
    assert_eq!(next_text(&mut ws).await, HELLO);
    wait_counts(&hub, |c| c.total == 1).await;
    // About 16 MB while the client reads nothing: the socket buffers fill,
    // the server task blocks on send, and the 16-message queue overflows.
    let pad = "x".repeat(2_000);
    const N: u64 = 8_000;
    for i in 0..N {
        hub.publish(format!(r#"{{"t":"test","seq":{i},"pad":"{pad}"}}"#));
    }
    let mut hellos = 0;
    let mut last: Option<u64> = None;
    let mut received = 0u64;
    loop {
        let t = next_text(&mut ws).await;
        if t == HELLO {
            hellos += 1;
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        let seq = v["seq"].as_u64().unwrap();
        if let Some(prev) = last {
            assert!(seq > prev, "order kept: {seq} after {prev}");
        }
        last = Some(seq);
        received += 1;
        if seq == N - 1 {
            break;
        }
    }
    assert!(hellos >= 1, "a resynchronizing hello was sent");
    assert!(received < N, "the backlog was dropped");
}

#[tokio::test]
async fn client_counts_follow_hello_and_disconnect() {
    let hub = hub();
    let server = start(hub.clone());
    let mut ws = ws_connect(server.port(), None, None).await.unwrap();
    next_text(&mut ws).await;
    wait_counts(&hub, |c| c.total == 1 && c.unidentified() == 1).await;
    ws.send(Message::text(
        r#"{"t":"hello","route":"keys","host":"obs"}"#,
    ))
    .await
    .unwrap();
    wait_counts(&hub, |c| c.obs == 1 && c.keys == 1).await;
    let mut ws2 = ws_connect(server.port(), None, None).await.unwrap();
    next_text(&mut ws2).await;
    ws2.send(Message::text(
        r#"{"t":"hello","route":"all","host":"browser"}"#,
    ))
    .await
    .unwrap();
    wait_counts(&hub, |c| c.total == 2 && c.browser == 1 && c.all == 1).await;
    let c = hub.counts();
    assert_eq!(c.route(Route::Keys), 1);
    assert_eq!(c.host(ClientHost::Obs), 1);
    assert_eq!(c.sources(), 2);
    ws.close(None).await.unwrap();
    wait_counts(&hub, |c| c.total == 1 && c.obs == 0 && c.keys == 0).await;
    drop(ws2);
    wait_counts(&hub, |c| c.total == 0).await;
}

#[tokio::test]
async fn too_many_clients_get_503() {
    let hub = hub();
    let server = start_with(
        hub.clone(),
        ServerConfig {
            port: 0,
            max_clients: 2,
        },
    );
    let mut a = ws_connect(server.port(), None, None).await.unwrap();
    let mut b = ws_connect(server.port(), None, None).await.unwrap();
    next_text(&mut a).await;
    next_text(&mut b).await;
    match ws_connect(server.port(), None, None).await {
        Err(WsError::Http(resp)) => assert_eq!(resp.status(), 503),
        other => panic!("expected 503, got {:?}", other.map(|_| ())),
    }
    drop(a);
    wait_counts(&hub, |c| c.total == 1).await;
    let mut c = ws_connect(server.port(), None, None).await.unwrap();
    assert_eq!(next_text(&mut c).await, HELLO);
}

/// A non-loopback address of this machine, if it has one.
fn lan_address() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // UDP connect sends nothing; it only picks the outgoing interface.
    s.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match s.local_addr().ok()? {
        SocketAddr::V4(a) if !a.ip().is_loopback() && !a.ip().is_unspecified() => Some(*a.ip()),
        _ => None,
    }
}

#[test]
fn binds_loopback_only_and_reports_a_busy_port() {
    let server = start(hub());
    assert_eq!(server.addr().ip(), Ipv4Addr::LOCALHOST);
    let port = server.port();
    assert!(StdTcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok());
    if let Some(ip) = lan_address() {
        let r =
            StdTcpStream::connect_timeout(&SocketAddr::from((ip, port)), Duration::from_secs(2));
        assert!(r.is_err(), "reachable on {ip}");
    }
    let v6 = StdTcpStream::connect_timeout(
        &SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port)),
        Duration::from_secs(2),
    );
    assert!(v6.is_err(), "reachable on [::1]");
    match Server::start(ServerConfig::new(port), assets(), hub()) {
        Err(ServerError::PortInUse { port: p }) => assert_eq!(p, port),
        other => panic!("expected PortInUse, got {other:?}"),
    }
}

#[test]
fn shutdown_closes_clients_and_frees_the_port() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let hub = hub();
    let server = start(hub.clone());
    let port = server.port();
    assert!(server.is_running());
    let mut ws = rt.block_on(async {
        let mut ws = ws_connect(port, None, None).await.unwrap();
        next_text(&mut ws).await;
        ws
    });
    let t0 = std::time::Instant::now();
    server.shutdown();
    assert!(t0.elapsed() < Duration::from_secs(5));
    let close = rt.block_on(async {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
                Ok(Some(Ok(Message::Close(frame)))) => return frame,
                Ok(Some(Ok(_))) => continue,
                other => panic!("expected a close frame, got {other:?}"),
            }
        }
    });
    assert_eq!(close.unwrap().code, CloseCode::Away);
    assert!(StdTcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
    assert_eq!(hub.counts().total, 0);
    // The port can be used again at once.
    let again = Server::start(ServerConfig::new(port), assets(), hub).unwrap();
    assert_eq!(again.port(), port);
}

#[test]
fn dir_assets_serve_a_dist_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(
        dir.path().join("overlay.html"),
        "<!doctype html><p>dist</p>",
    )
    .unwrap();
    std::fs::write(dir.path().join("assets").join("o-1.js"), "1").unwrap();
    let assets = Arc::new(keystage_server::DirAssets::new(dir.path()).unwrap());
    let server = Server::start(ServerConfig::new(0), assets, hub()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let r = http(server.port(), "GET", "/overlay/all", &[]).await;
        assert_eq!(r.body, b"<!doctype html><p>dist</p>");
        let r = http(server.port(), "GET", "/assets/o-1.js", &[]).await;
        assert_eq!(r.status, 200);
    });
}

#[test]
fn missing_overlay_html_is_a_clear_500() {
    let server = Server::start(ServerConfig::new(0), Arc::new(MemoryAssets::new()), hub()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let r = http(server.port(), "GET", "/overlay/keys", &[]).await;
        assert_eq!(r.status, 500);
        assert_eq!(
            String::from_utf8(r.body).unwrap(),
            "The overlay page is missing from this build."
        );
    });
}
