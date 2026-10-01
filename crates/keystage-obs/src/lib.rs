//! keystage-obs: a small obs-websocket 5 client (OBS 28 and later).
//!
//! * [`ObsClient`] connects, authenticates (SHA-256 and base64 as the
//!   protocol documents), subscribes to General and Outputs events only,
//!   follows the recording state (from `outputState`, seeded with
//!   `GetRecordStatus`), polls the timecode once a second while recording,
//!   mirrors chapter markers into OBS when the recording format supports
//!   them, and reconnects with backoff. Its state is on a tokio watch
//!   channel ([`ObsStatus`]) and its events on a broadcast channel
//!   ([`ObsEvent`]); every method is synchronous.
//! * [`test_connection`] is the settings window's one-off check.
//!
//! The only socket it opens is to the host the user configured (default
//! `127.0.0.1:4455`). The password is passed in by the caller (the app
//! decrypts it with DPAPI) and is never logged; neither are chapter names.
//!
//! Protocol facts and sources: `.work/research/obs.md` section B and
//! docs/DECISIONS.md item 5.

pub mod auth;
pub mod client;
pub mod protocol;
pub mod status;

pub use auth::{auth_string, Password};
pub use client::{
    test_connection, test_connection_blocking, ClientOptions, ObsClient, ObsConfig, DEFAULT_HOST,
    DEFAULT_PORT,
};
pub use status::{
    ChapterOutcome, ChapterSupport, Connection, DurationSample, ObsEvent, ObsStatus, RecordState,
    Recording,
};
