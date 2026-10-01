//! keystage-server: the local server OBS Browser Sources load the overlay
//! from (ARCHITECTURE.md section 6).
//!
//! * Bound to `127.0.0.1` only (default port 47820).
//! * `GET /overlay/{keys|keyboard|mouse|cursor|all}` serves `overlay.html`;
//!   `/assets/*`, `/fonts/*` and `/sounds/*` serve the built front end from
//!   an [`Assets`] source; `GET /api/health` reports the app and protocol;
//!   `GET /ws` is the event WebSocket. Everything else is 404. No CORS
//!   headers.
//! * The WebSocket is accepted only when `Host` is `127.0.0.1:<port>` or
//!   `localhost:<port>` and `Origin` is absent or one of
//!   `http://127.0.0.1:<port>`, `http://localhost:<port>`; anything else is
//!   403. This keeps web pages and DNS rebinding away from keystrokes.
//! * The [`Hub`] fans pre-serialized messages out to every client, gives a
//!   connecting or lagging client a fresh `hello`, and counts clients by
//!   route and host.
//!
//! Logging is lifecycle only: no message content, no key names.

pub mod assets;
pub mod hub;
pub mod server;
mod ws;

pub use assets::{Asset, Assets, DirAssets, MemoryAssets};
pub use hub::{ClientCounts, ClientInfo, Hub, DEFAULT_CAPACITY};
pub use server::{
    overlay_url, Server, ServerConfig, ServerError, ServerHandle, DEFAULT_MAX_CLIENTS,
};
