//! The local server for OBS Browser Sources (ARCHITECTURE.md section 6):
//! keystage-server on `settings.server.port`, serving the front end the app
//! embeds (through Tauri's `AssetResolver`), restarted when the port
//! changes. A busy or blocked port is reported in `Status.server.error` in
//! the server crate's own words.

use std::sync::Arc;

use keystage_core::ipc::ServerStatus;
use keystage_server::{Asset, Assets, Hub, Server, ServerConfig, ServerHandle};
use parking_lot::Mutex;
use tauri::{AssetResolver, Runtime};

/// Serves the files Tauri embedded from `dist/`.
pub struct TauriAssets<R: Runtime> {
    resolver: AssetResolver<R>,
}

impl<R: Runtime> TauriAssets<R> {
    pub fn new(resolver: AssetResolver<R>) -> TauriAssets<R> {
        TauriAssets { resolver }
    }
}

/// Tauri's resolver falls back to `<path>.html`, `<path>/index.html` and
/// `index.html` when a file is missing. The server must answer 404 for a
/// missing file instead, so an HTML answer to a request for anything but an
/// HTML file is refused.
pub fn accept_resolved(requested: &str, mime: &str) -> bool {
    let wants_html = requested.ends_with(".html") || requested.ends_with(".htm");
    let got_html = mime.trim().to_ascii_lowercase().starts_with("text/html");
    wants_html || !got_html
}

impl<R: Runtime> Assets for TauriAssets<R> {
    fn get(&self, path: &str) -> Option<Asset> {
        let a = self.resolver.get(path.to_string())?;
        if !accept_resolved(path, &a.mime_type) {
            return None;
        }
        Some(Asset {
            bytes: a.bytes.into(),
            mime: Some(a.mime_type.into()),
        })
    }
}

#[derive(Debug)]
struct Inner {
    handle: Option<ServerHandle>,
    port: u16,
    error: Option<String>,
}

/// Starts, restarts and reports the server.
pub struct ServerManager {
    hub: Hub,
    assets: Arc<dyn Assets>,
    inner: Mutex<Inner>,
}

impl ServerManager {
    pub fn new(hub: Hub, assets: Arc<dyn Assets>) -> ServerManager {
        ServerManager {
            hub,
            assets,
            inner: Mutex::new(Inner {
                handle: None,
                port: 0,
                error: None,
            }),
        }
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }

    /// Serves on `port`, stopping a server on another port first. Nothing
    /// happens when it already runs there. Blocks for a few seconds at most
    /// (the old server closes its sockets); call it from a plain thread.
    pub fn ensure(&self, port: u16) {
        let mut inner = self.inner.lock();
        if inner.port == port && inner.handle.as_ref().is_some_and(|h| h.is_running()) {
            return;
        }
        if let Some(old) = inner.handle.take() {
            old.shutdown();
        }
        inner.port = port;
        match Server::start(
            ServerConfig::new(port),
            self.assets.clone(),
            self.hub.clone(),
        ) {
            Ok(h) => {
                inner.port = h.port();
                inner.handle = Some(h);
                inner.error = None;
            }
            Err(e) => {
                tracing::warn!(port, "local server did not start");
                inner.error = Some(e.to_string());
            }
        }
    }

    /// Stops the server.
    pub fn stop(&self) {
        let old = self.inner.lock().handle.take();
        if let Some(h) = old {
            h.shutdown();
        }
    }

    pub fn port(&self) -> u16 {
        self.inner.lock().port
    }

    pub fn running(&self) -> bool {
        self.inner
            .lock()
            .handle
            .as_ref()
            .is_some_and(|h| h.is_running())
    }

    /// `Status.server`.
    pub fn status(&self) -> ServerStatus {
        let inner = self.inner.lock();
        let running = inner.handle.as_ref().is_some_and(|h| h.is_running());
        let error = match (&inner.error, running, &inner.handle) {
            (Some(e), _, _) => Some(e.clone()),
            (None, false, Some(_)) => Some("The local server stopped unexpectedly.".into()),
            _ => None,
        };
        ServerStatus {
            port: inner.port,
            running,
            error,
            clients: self.hub.counts().total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_server::MemoryAssets;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn assets() -> Arc<dyn Assets> {
        let mut m = MemoryAssets::new();
        m.insert("overlay.html", b"<!doctype html><title>t</title>".to_vec());
        Arc::new(m)
    }

    fn health(port: u16) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        write!(
            s,
            "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )
        .expect("write");
        let mut out = String::new();
        s.read_to_string(&mut out).expect("read");
        out
    }

    #[test]
    fn html_fallbacks_are_refused() {
        assert!(accept_resolved("overlay.html", "text/html"));
        assert!(accept_resolved("assets/a.js", "text/javascript"));
        assert!(!accept_resolved("assets/missing.js", "text/html"));
        assert!(!accept_resolved(
            "fonts/x.woff2",
            "TEXT/HTML; charset=utf-8"
        ));
        assert!(accept_resolved("assets/x.woff2", "font/woff2"));
    }

    #[test]
    fn starts_restarts_and_reports_a_busy_port() {
        let m = ServerManager::new(Hub::new(), assets());
        // Port 0 picks a free port.
        m.ensure(0);
        let first = m.port();
        assert!(first > 0);
        assert!(m.running());
        let st = m.status();
        assert!(st.running && st.error.is_none());
        assert!(health(first).contains("\"ok\":true"));
        // Same port: nothing changes.
        m.ensure(first);
        assert_eq!(m.port(), first);
        // A second manager on the same port reports it in plain words.
        let other = ServerManager::new(Hub::new(), assets());
        other.ensure(first);
        let st = other.status();
        assert!(!st.running);
        let err = st.error.expect("an error");
        assert!(err.contains(&first.to_string()), "{err}");
        assert!(err.contains("already in use"), "{err}");
        // Moving to another port frees the old one.
        m.ensure(0);
        m.stop();
        assert!(!m.running());
        other.ensure(first);
        assert!(other.running(), "the old port is free again");
        other.stop();
    }
}
