//! The Hub: fans pre-serialized protocol messages out to every WebSocket
//! client, provides the `hello` snapshot a client gets on connect (and again
//! after it falls behind), and counts clients by route and host.
//!
//! The Hub is cheap to clone and every method is synchronous, so the app's
//! pipeline thread can publish without touching async code.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use keystage_core::protocol::{AppState, ClientHost, OverlayConfig, Route, ServerMsg};
use keystage_core::DisplayEvent;
use serde::Serialize;
use tokio::sync::{broadcast, watch};

/// Messages a client may fall behind by before it is resynchronized with a
/// fresh `hello`.
pub const DEFAULT_CAPACITY: usize = 1024;

type SnapshotFn = dyn Fn() -> String + Send + Sync;

/// Connected clients, by host and by route, from each client's `hello`.
/// A client that has not sent its `hello` yet counts only in `total`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub struct ClientCounts {
    pub total: u32,
    pub obs: u32,
    pub browser: u32,
    pub native: u32,
    pub preview: u32,
    pub keys: u32,
    pub keyboard: u32,
    pub mouse: u32,
    pub cursor: u32,
    pub all: u32,
}

impl ClientCounts {
    /// Clients that said they run in this host.
    pub fn host(&self, host: ClientHost) -> u32 {
        match host {
            ClientHost::Obs => self.obs,
            ClientHost::Browser => self.browser,
            ClientHost::Native => self.native,
            ClientHost::Preview => self.preview,
        }
    }

    /// Clients that show this route.
    pub fn route(&self, route: Route) -> u32 {
        match route {
            Route::Keys => self.keys,
            Route::Keyboard => self.keyboard,
            Route::Mouse => self.mouse,
            Route::Cursor => self.cursor,
            Route::All => self.all,
        }
    }

    /// OBS Browser Sources plus ordinary browser pages: `AppState.obs_clients`.
    pub fn sources(&self) -> u32 {
        self.obs + self.browser
    }

    /// Clients that have not said who they are yet.
    pub fn unidentified(&self) -> u32 {
        self.total
            .saturating_sub(self.obs + self.browser + self.native + self.preview)
    }
}

/// One connected client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientInfo {
    pub id: u64,
    pub route: Option<Route>,
    pub host: Option<ClientHost>,
    pub connected: Instant,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    route: Option<Route>,
    host: Option<ClientHost>,
    connected: Instant,
}

struct Inner {
    tx: broadcast::Sender<Arc<str>>,
    snapshot: RwLock<Arc<SnapshotFn>>,
    clients: Mutex<HashMap<u64, Entry>>,
    counts: watch::Sender<ClientCounts>,
    next_id: AtomicU64,
}

/// The message hub shared by the app and the server.
#[derive(Clone)]
pub struct Hub {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Hub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hub")
            .field("counts", &self.counts())
            .finish_non_exhaustive()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Hub::new()
    }
}

fn default_hello() -> String {
    ServerMsg::hello(OverlayConfig::default(), AppState::default(), None).to_json()
}

impl Hub {
    /// A hub with [`DEFAULT_CAPACITY`] and a default `hello` (default
    /// config and state) until [`Hub::set_snapshot`] is called.
    pub fn new() -> Hub {
        Hub::with_capacity(DEFAULT_CAPACITY)
    }

    /// A hub whose clients may fall `capacity` messages behind before they
    /// are resynchronized.
    pub fn with_capacity(capacity: usize) -> Hub {
        let (tx, _) = broadcast::channel(capacity.max(1));
        let (counts, _) = watch::channel(ClientCounts::default());
        let snapshot: Arc<SnapshotFn> = Arc::new(default_hello);
        Hub {
            inner: Arc::new(Inner {
                tx,
                snapshot: RwLock::new(snapshot),
                clients: Mutex::new(HashMap::new()),
                counts,
                next_id: AtomicU64::new(1),
            }),
        }
    }

    /// Sets the function that builds the full `hello` JSON (current config,
    /// state and legends). It is called on every connect and whenever a
    /// client must be resynchronized, from a server worker thread, so it
    /// should only read shared state and serialize.
    pub fn set_snapshot<F>(&self, provider: F)
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        let f: Arc<SnapshotFn> = Arc::new(provider);
        match self.inner.snapshot.write() {
            Ok(mut slot) => *slot = f,
            Err(poisoned) => *poisoned.into_inner() = f,
        }
    }

    /// The `hello` a client would get right now.
    pub fn snapshot(&self) -> Arc<str> {
        let f = match self.inner.snapshot.read() {
            Ok(slot) => Arc::clone(&slot),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        };
        Arc::from(f())
    }

    /// Sends one serialized message to every connected client, in order.
    /// Returns how many clients it was queued for (0 when none are
    /// connected; nothing is kept for later clients).
    pub fn publish(&self, json: impl Into<Arc<str>>) -> usize {
        self.inner.tx.send(json.into()).unwrap_or(0)
    }

    /// Serializes and publishes a protocol message.
    pub fn publish_msg(&self, msg: &ServerMsg) -> usize {
        if self.inner.tx.receiver_count() == 0 {
            return 0;
        }
        self.publish(msg.to_json())
    }

    /// Serializes and publishes a processor display event.
    pub fn publish_event(&self, ev: &DisplayEvent) -> usize {
        if self.inner.tx.receiver_count() == 0 {
            return 0;
        }
        self.publish(ev.to_json())
    }

    /// Current client counts.
    pub fn counts(&self) -> ClientCounts {
        *self.inner.counts.borrow()
    }

    /// A watch on the client counts; it changes whenever a client connects,
    /// identifies itself or leaves.
    pub fn watch_counts(&self) -> watch::Receiver<ClientCounts> {
        self.inner.counts.subscribe()
    }

    /// The connected clients.
    pub fn clients(&self) -> Vec<ClientInfo> {
        let map = self.lock_clients();
        let mut list: Vec<ClientInfo> = map
            .iter()
            .map(|(&id, e)| ClientInfo {
                id,
                route: e.route,
                host: e.host,
                connected: e.connected,
            })
            .collect();
        list.sort_by_key(|c| c.id);
        list
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Arc<str>> {
        self.inner.tx.subscribe()
    }

    /// Registers a client; it is removed when the guard drops.
    pub(crate) fn register(&self) -> ClientGuard {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.lock_clients().insert(
            id,
            Entry {
                route: None,
                host: None,
                connected: Instant::now(),
            },
        );
        self.recount();
        ClientGuard {
            hub: self.clone(),
            id,
        }
    }

    fn lock_clients(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Entry>> {
        match self.inner.clients.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn recount(&self) {
        let counts = {
            let map = self.lock_clients();
            let mut c = ClientCounts {
                total: u32::try_from(map.len()).unwrap_or(u32::MAX),
                ..ClientCounts::default()
            };
            for e in map.values() {
                match e.host {
                    Some(ClientHost::Obs) => c.obs += 1,
                    Some(ClientHost::Browser) => c.browser += 1,
                    Some(ClientHost::Native) => c.native += 1,
                    Some(ClientHost::Preview) => c.preview += 1,
                    None => {}
                }
                match e.route {
                    Some(Route::Keys) => c.keys += 1,
                    Some(Route::Keyboard) => c.keyboard += 1,
                    Some(Route::Mouse) => c.mouse += 1,
                    Some(Route::Cursor) => c.cursor += 1,
                    Some(Route::All) => c.all += 1,
                    None => {}
                }
            }
            c
        };
        self.inner.counts.send_if_modified(|cur| {
            if *cur == counts {
                false
            } else {
                *cur = counts;
                true
            }
        });
    }
}

/// A registered client. Dropping it unregisters the client.
pub(crate) struct ClientGuard {
    hub: Hub,
    id: u64,
}

impl ClientGuard {
    /// Records the route and host from the client's `hello`.
    pub(crate) fn identify(&self, route: Route, host: ClientHost) {
        let changed = {
            let mut map = self.hub.lock_clients();
            match map.get_mut(&self.id) {
                Some(e) if e.route != Some(route) || e.host != Some(host) => {
                    e.route = Some(route);
                    e.host = Some(host);
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.hub.recount();
        }
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.hub.lock_clients().remove(&self.id);
        self.hub.recount();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_follow_registration_and_identity() {
        let hub = Hub::new();
        let mut watch = hub.watch_counts();
        assert_eq!(hub.counts(), ClientCounts::default());
        let a = hub.register();
        let b = hub.register();
        assert!(watch.has_changed().unwrap());
        assert_eq!(watch.borrow_and_update().total, 2);
        a.identify(Route::Keys, ClientHost::Obs);
        b.identify(Route::All, ClientHost::Browser);
        let c = hub.counts();
        assert_eq!(c.total, 2);
        assert_eq!(c.host(ClientHost::Obs), 1);
        assert_eq!(c.host(ClientHost::Browser), 1);
        assert_eq!(c.route(Route::Keys), 1);
        assert_eq!(c.route(Route::All), 1);
        assert_eq!(c.sources(), 2);
        assert_eq!(c.unidentified(), 0);
        let list = hub.clients();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].route, Some(Route::Keys));
        // Identifying again with the same values changes nothing.
        watch.borrow_and_update();
        a.identify(Route::Keys, ClientHost::Obs);
        assert!(!watch.has_changed().unwrap());
        // A client may change its route (a reloaded page).
        a.identify(Route::Keyboard, ClientHost::Obs);
        assert_eq!(hub.counts().keyboard, 1);
        assert_eq!(hub.counts().keys, 0);
        drop(a);
        let c = hub.counts();
        assert_eq!((c.total, c.obs, c.keyboard), (1, 0, 0));
        let pending = hub.register();
        assert_eq!(hub.counts().unidentified(), 1);
        drop(pending);
        drop(b);
        assert_eq!(hub.counts(), ClientCounts::default());
    }

    #[test]
    fn snapshot_defaults_and_provider() {
        let hub = Hub::new();
        let v: serde_json::Value = serde_json::from_str(&hub.snapshot()).unwrap();
        assert_eq!(v["t"], "hello");
        assert_eq!(v["v"], 1);
        hub.set_snapshot(|| String::from("{\"t\":\"hello\",\"x\":1}"));
        assert_eq!(&*hub.snapshot(), "{\"t\":\"hello\",\"x\":1}");
    }

    #[test]
    fn publish_without_clients_is_dropped() {
        let hub = Hub::with_capacity(4);
        assert_eq!(hub.publish("{\"t\":\"clear\"}"), 0);
        assert_eq!(hub.publish_msg(&ServerMsg::Clear), 0);
        let mut rx = hub.subscribe();
        assert_eq!(hub.publish_msg(&ServerMsg::Clear), 1);
        assert_eq!(
            hub.publish_event(&DisplayEvent::ModifiersReleased { t_ms: 5 }),
            1
        );
        assert_eq!(&*rx.try_recv().unwrap(), "{\"t\":\"clear\"}");
        assert_eq!(
            &*rx.try_recv().unwrap(),
            "{\"t\":\"release_all\",\"t_ms\":5}"
        );
    }
}
