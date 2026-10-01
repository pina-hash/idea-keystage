//! keystage-core: the platform-independent heart of IDEA Keystage.
//!
//! * [`Processor`] turns raw keyboard and mouse events into display events
//!   (combos, repeats, chords, text runs, masking, captions, suppression).
//! * [`protocol`] is the wire format both overlay hosts receive.
//! * [`profile`] reads the sourced SolidWorks data and finds captions;
//!   [`import`] reads the user's own shortcut exports.
//! * [`settings`] is the stored configuration; [`ipc`] the settings window types.
//! * [`synth`] renders the original key sounds.
//!
//! The crate does no IO, starts no threads, and makes no OS calls. Nothing in
//! it logs. See `docs/ARCHITECTURE.md` sections 3, 4, 5, 7, 9 and 12.

pub mod builtin;
pub mod event;
pub mod import;
pub mod ipc;
pub mod keys;
pub mod legends;
pub mod modifiers;
pub mod processor;
pub mod profile;
pub mod protocol;
pub mod rules;
mod serde_util;
pub mod settings;
pub mod synth;

pub use event::{
    Callout, CalloutKind, Chip, ChipKind, DisplayEvent, InputEvent, KeyInput, Mod, MonitorRect,
    MouseButton, PhysKey, Side, Suppress,
};
pub use import::{ImportFormat, ImportPreview, ImportResult, UserShortcut};
pub use legends::{LegendProvider, NoLegends, TextMods, UsLegends};
pub use modifiers::{ModKey, ModSet};
pub use processor::{Processor, Stats};
pub use profile::{CaptionIndex, Profile};
pub use protocol::{AppState, ClientMsg, Legends, OverlayConfig, ServerMsg};
pub use rules::{DisplayRules, TypingMode};
pub use settings::Settings;
