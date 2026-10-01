//! The HTTP and WebSocket server (ARCHITECTURE.md section 6). It runs on its
//! own tokio runtime (two worker threads) inside a plain thread, so the
//! caller needs no async code.

use std::future::IntoFuture;
use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdListener};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::serve::ListenerExt;
use axum::Router;
use keystage_core::protocol::{Route, APP_NAME, APP_VERSION, DEFAULT_PORT, PROTOCOL_VERSION};
use serde::Serialize;
use tokio::sync::watch;

use crate::assets::{self, Assets};
use crate::hub::Hub;
use crate::ws;

/// Most WebSocket clients at once; more get 503.
pub const DEFAULT_MAX_CLIENTS: usize = 64;
/// How long a graceful shutdown waits for open requests.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// Server settings. The server always binds `127.0.0.1`; there is no
/// option to listen anywhere else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    /// TCP port; 0 picks a free one (tests).
    pub port: u16,
    /// Most WebSocket clients at once.
    pub max_clients: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            port: DEFAULT_PORT,
            max_clients: DEFAULT_MAX_CLIENTS,
        }
    }
}

impl ServerConfig {
    pub fn new(port: u16) -> ServerConfig {
        ServerConfig {
            port,
            ..ServerConfig::default()
        }
    }
}

/// Why the server did not start. The messages are written for the
/// settings window.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Port {port} is already in use by another program. Close that program or choose another port.")]
    PortInUse { port: u16 },
    #[error("Windows does not allow Keystage to use port {port}. Choose another port.")]
    PortBlocked { port: u16 },
    #[error("Could not open port {port} on 127.0.0.1: {source}")]
    Bind {
        port: u16,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not start the local server: {0}")]
    Runtime(#[source] std::io::Error),
}

impl ServerError {
    fn from_bind(port: u16, e: std::io::Error) -> ServerError {
        match e.kind() {
            std::io::ErrorKind::AddrInUse => ServerError::PortInUse { port },
            // Windows answers WSAEACCES for ports inside an excluded range.
            std::io::ErrorKind::PermissionDenied => ServerError::PortBlocked { port },
            _ => ServerError::Bind { port, source: e },
        }
    }
}

/// The OBS Browser Source URL for a route on a port.
pub fn overlay_url(port: u16, route: Route) -> String {
    let name = match route {
        Route::Keys => "keys",
        Route::Keyboard => "keyboard",
        Route::Mouse => "mouse",
        Route::Cursor => "cursor",
        Route::All => "all",
    };
    format!("http://127.0.0.1:{port}/overlay/{name}")
}

/// Starts servers. See [`Server::start`].
pub struct Server;

impl Server {
    /// Binds `127.0.0.1:<port>` and serves the overlay routes and the event
    /// WebSocket on a new thread with its own runtime. Returns once the port
    /// is bound; a busy port is reported here, not later.
    pub fn start(
        config: ServerConfig,
        assets: Arc<dyn Assets>,
        hub: Hub,
    ) -> Result<ServerHandle, ServerError> {
        let requested = SocketAddr::from((Ipv4Addr::LOCALHOST, config.port));
        let listener =
            StdListener::bind(requested).map_err(|e| ServerError::from_bind(config.port, e))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| ServerError::Bind {
                port: config.port,
                source: e,
            })?;
        let addr = listener.local_addr().map_err(|e| ServerError::Bind {
            port: config.port,
            source: e,
        })?;
        let (stop_tx, stop_rx) = watch::channel(false);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<std::io::Result<()>>(1);
        let state = AppState {
            port: addr.port(),
            assets,
            hub,
            stop: stop_rx.clone(),
            max_clients: config.max_clients.max(1),
        };
        let thread = thread::Builder::new()
            .name("keystage-server".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("keystage-server-worker")
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let listener = {
                    let _enter = rt.enter();
                    match tokio::net::TcpListener::from_std(listener) {
                        Ok(l) => l,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                    }
                };
                let _ = ready_tx.send(Ok(()));
                tracing::info!(port = addr.port(), "local server listening on 127.0.0.1");
                rt.block_on(serve(listener, state, stop_rx));
                rt.shutdown_timeout(SHUTDOWN_GRACE);
                tracing::info!("local server stopped");
            })
            .map_err(ServerError::Runtime)?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(ServerHandle {
                addr,
                stop: stop_tx,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(ServerError::Runtime(e))
            }
            Err(_) => {
                let _ = thread.join();
                Err(ServerError::Runtime(std::io::Error::other(
                    "the server thread ended before it was ready",
                )))
            }
        }
    }
}

/// A running server. Dropping it shuts the server down.
pub struct ServerHandle {
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("addr", &self.addr)
            .field("running", &self.is_running())
            .finish()
    }
}

impl ServerHandle {
    /// The bound port (the real one when 0 was requested).
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// The bound address, always `127.0.0.1:<port>`.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// False once the server thread has ended.
    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Stops accepting connections, closes every WebSocket with code 1001
    /// ("going away"), and waits (a few seconds at most) for the server
    /// thread to end. Call it from a plain thread or a blocking context.
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        let _ = self.stop.send(true);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) port: u16,
    pub(crate) assets: Arc<dyn Assets>,
    pub(crate) hub: Hub,
    pub(crate) stop: watch::Receiver<bool>,
    pub(crate) max_clients: usize,
}

async fn serve(
    listener: tokio::net::TcpListener,
    state: AppState,
    mut stop: watch::Receiver<bool>,
) {
    let app = router(state);
    let listener = listener.tap_io(|tcp| {
        // Small frames must leave at once (Nagle plus delayed ACK can hold
        // them back on Windows loopback).
        let _ = tcp.set_nodelay(true);
    });
    let mut stop_signal = stop.clone();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = stop_signal.wait_for(|s| *s).await;
    });
    let mut server = std::pin::pin!(server.into_future());
    tokio::select! {
        r = &mut server => {
            if let Err(e) = r {
                tracing::error!(error = %e, "local server failed");
            }
        }
        _ = async {
            let _ = stop.wait_for(|s| *s).await;
            tokio::time::sleep(SHUTDOWN_GRACE).await;
        } => {
            tracing::warn!("local server shutdown timed out; closing open connections");
        }
    }
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/overlay/{route}", get(overlay))
        .route("/assets/{*path}", get(static_file))
        .route("/fonts/{*path}", get(static_file))
        .route("/sounds/{*path}", get(static_file))
        .route("/ws", get(websocket))
        .route("/api/health", get(health))
        .fallback(not_found)
        .method_not_allowed_fallback(not_found)
        .with_state(state)
}

fn text_response(status: StatusCode, body: &'static str) -> Response {
    let mut r = Response::new(Body::from(body));
    *r.status_mut() = status;
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

async fn not_found() -> Response {
    text_response(StatusCode::NOT_FOUND, "Not found")
}

fn forbidden() -> Response {
    text_response(StatusCode::FORBIDDEN, "Forbidden")
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    app: &'static str,
    version: &'static str,
    protocol: u32,
}

async fn health() -> Response {
    let body = serde_json::to_string(&Health {
        ok: true,
        app: APP_NAME,
        version: APP_VERSION,
        protocol: PROTOCOL_VERSION,
    })
    .unwrap_or_else(|_| String::from("{\"ok\":true}"));
    let mut r = Response::new(Body::from(body));
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

/// The overlay page's Content Security Policy: everything from this origin,
/// the socket on either loopback name, inline styles (Svelte sets style
/// attributes), nothing else.
pub(crate) fn overlay_csp(port: u16) -> String {
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; font-src 'self' data:; \
         connect-src 'self' ws://127.0.0.1:{port} ws://localhost:{port}; \
         object-src 'none'; base-uri 'none'; form-action 'none'"
    )
}

async fn load(state: &AppState, path: String) -> Option<(String, assets::Asset)> {
    let src = Arc::clone(&state.assets);
    tokio::task::spawn_blocking(move || src.get(&path).map(|a| (path, a)))
        .await
        .ok()
        .flatten()
}

fn file_response(path: &str, asset: assets::Asset, cache: &'static str) -> Response {
    let ctype = assets::content_type(path, asset.mime.as_deref());
    let mut r = Response::new(Body::from(asset.bytes));
    let h = r.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&ctype) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

async fn overlay(State(state): State<AppState>, Path(route): Path<String>) -> Response {
    if !matches!(
        route.as_str(),
        "keys" | "keyboard" | "mouse" | "cursor" | "all"
    ) {
        return not_found().await;
    }
    match load(&state, "overlay.html".to_string()).await {
        Some((path, asset)) => {
            let mut r = file_response(&path, asset, "no-store");
            let h = r.headers_mut();
            if let Ok(v) = HeaderValue::from_str(&overlay_csp(state.port)) {
                h.insert(header::CONTENT_SECURITY_POLICY, v);
            }
            h.insert(
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            );
            r
        }
        None => {
            tracing::error!("overlay.html is missing from the front-end assets");
            text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The overlay page is missing from this build.",
            )
        }
    }
}

async fn static_file(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();
    if !assets::is_safe_path(&path) {
        return not_found().await;
    }
    // Vite names files under assets/ by content hash, so they never change.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    match load(&state, path).await {
        Some((path, asset)) => file_response(&path, asset, cache),
        None => not_found().await,
    }
}

/// Strips `:80` (the only port a browser leaves out for `http`).
fn authority_matches(value: &str, port: u16) -> bool {
    let v = value.trim().to_ascii_lowercase();
    ["127.0.0.1", "localhost"]
        .iter()
        .any(|name| v == format!("{name}:{port}") || (port == 80 && v == *name))
}

/// Section 6: the `Host` header must be `127.0.0.1:<port>` or
/// `localhost:<port>` (this blocks DNS rebinding).
pub(crate) fn host_allowed(headers: &HeaderMap, port: u16) -> bool {
    let mut values = headers.get_all(header::HOST).iter();
    match (values.next(), values.next()) {
        (Some(v), None) => v.to_str().is_ok_and(|s| authority_matches(s, port)),
        _ => false,
    }
}

/// Section 6: the `Origin` header must be absent, or exactly
/// `http://127.0.0.1:<port>` or `http://localhost:<port>` (this keeps
/// ordinary web pages from reading keystrokes).
pub(crate) fn origin_allowed(headers: &HeaderMap, port: u16) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    match (values.next(), values.next()) {
        (None, _) => true,
        (Some(v), None) => v.to_str().is_ok_and(|s| {
            let s = s.trim();
            s.len() > 7
                && s[..7].eq_ignore_ascii_case("http://")
                && authority_matches(&s[7..], port)
        }),
        _ => false,
    }
}

async fn websocket(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    if method != Method::GET {
        return not_found().await;
    }
    if !host_allowed(&headers, state.port) || !origin_allowed(&headers, state.port) {
        tracing::debug!("refused a WebSocket that failed the Host or Origin check");
        return forbidden();
    }
    let upgrade = match upgrade {
        Ok(u) => u,
        Err(rejection) => return rejection.into_response(),
    };
    if state.hub.counts().total as usize >= state.max_clients {
        tracing::debug!("refused a WebSocket: too many clients");
        return text_response(StatusCode::SERVICE_UNAVAILABLE, "Too many connections");
    }
    let guard = state.hub.register();
    let hub = state.hub.clone();
    let stop = state.stop.clone();
    upgrade
        .max_message_size(ws::MAX_CLIENT_MESSAGE)
        .max_frame_size(ws::MAX_CLIENT_MESSAGE)
        .on_upgrade(move |socket| ws::run(socket, hub, guard, stop))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn host_rule() {
        let port = 47820;
        for ok in ["127.0.0.1:47820", "localhost:47820", "LOCALHOST:47820"] {
            assert!(host_allowed(&headers(&[("host", ok)]), port), "{ok}");
        }
        for bad in [
            "attacker.example:47820",
            "127.0.0.1",
            "127.0.0.1:47821",
            "127.0.0.2:47820",
            "[::1]:47820",
            "0.0.0.0:47820",
            "localhost.attacker.example:47820",
            "",
        ] {
            assert!(!host_allowed(&headers(&[("host", bad)]), port), "{bad}");
        }
        assert!(!host_allowed(&HeaderMap::new(), port), "missing");
        assert!(!host_allowed(
            &headers(&[("host", "127.0.0.1:47820"), ("host", "127.0.0.1:47820")]),
            port
        ));
        // Port 80 may be left out by the browser.
        assert!(host_allowed(&headers(&[("host", "localhost")]), 80));
    }

    #[test]
    fn origin_rule() {
        let port = 47820;
        assert!(origin_allowed(&HeaderMap::new(), port), "absent");
        for ok in [
            "http://127.0.0.1:47820",
            "http://localhost:47820",
            "HTTP://LocalHost:47820",
        ] {
            assert!(origin_allowed(&headers(&[("origin", ok)]), port), "{ok}");
        }
        for bad in [
            "http://evil.example",
            "https://127.0.0.1:47820",
            "http://127.0.0.1:47821",
            "http://127.0.0.1",
            "null",
            "http://absolute",
            "file://",
            "http://127.0.0.1:47820/",
            "tauri://localhost",
            "",
        ] {
            assert!(!origin_allowed(&headers(&[("origin", bad)]), port), "{bad}");
        }
        assert!(!origin_allowed(
            &headers(&[
                ("origin", "http://127.0.0.1:47820"),
                ("origin", "http://evil.example")
            ]),
            port
        ));
    }

    #[test]
    fn urls_and_errors() {
        assert_eq!(
            overlay_url(47820, Route::Keys),
            "http://127.0.0.1:47820/overlay/keys"
        );
        assert_eq!(overlay_url(1, Route::All), "http://127.0.0.1:1/overlay/all");
        let e = ServerError::from_bind(47820, std::io::Error::from(std::io::ErrorKind::AddrInUse));
        assert!(matches!(e, ServerError::PortInUse { port: 47820 }));
        assert_eq!(
            e.to_string(),
            "Port 47820 is already in use by another program. Close that program or choose another port."
        );
        let e = ServerError::from_bind(
            50000,
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        );
        assert!(matches!(e, ServerError::PortBlocked { port: 50000 }));
        let e = ServerError::from_bind(1, std::io::Error::other("x"));
        assert!(matches!(e, ServerError::Bind { port: 1, .. }));
        assert_eq!(ServerConfig::default().port, 47820);
        assert!(overlay_csp(47820)
            .contains("connect-src 'self' ws://127.0.0.1:47820 ws://localhost:47820;"));
    }
}
