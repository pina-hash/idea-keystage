//! keystage-session: the Session Log and its exports (ARCHITECTURE.md
//! section 10).
//!
//! * [`SessionLog`] records a live session: the final state of every
//!   callout that was displayed, chapter markers, and what OBS reported
//!   about the recording (record state changes and duration samples).
//! * [`RecordingClock`] maps the app's monotonic milliseconds to video time,
//!   from OBS timing when available, otherwise from the session's own wall
//!   time.
//! * [`export`] turns a [`Session`] into SubRip captions, a Markdown list of
//!   shortcuts, the session JSON (`keystage.session/1`) and YouTube chapter
//!   lines.
//!
//! The crate does no IO except [`Session::save`] and [`Session::load`],
//! which the app calls when the user's settings say so. It reads no clock:
//! callers pass every time in. Nothing in it logs.

pub mod clock;
pub mod export;
pub mod session;
pub mod time;

pub use clock::{
    ClockEvent, ClockSample, Mapped, Position, RecordEvent, RecordingClock, RecordingLog,
};
pub use export::{Export, ExportOptions, ExportWarning};
pub use keystage_core::ipc::ExportFormat;
pub use session::{AppInfo, Marker, Session, SessionError, SessionLog, SESSION_SCHEMA};
