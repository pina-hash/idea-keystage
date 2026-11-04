//! keystage-automation: the platform-independent half of the IDEA Keystage
//! connector (docs/ARCHITECTURE.md section 14). No IO beyond the path check
//! reading file metadata, no threads, no OS calls, nothing logs.
//!
//! * [`motion`]: human-like cursor paths and `SendInput` coordinates.
//! * [`keys`]: chord and typing plans.
//! * [`script`]: `perform` scripts and their validation.
//! * [`abort`]: the watch that stops a run when a person takes over.
//! * [`occlude`] and [`spans`]: clean takes.
//! * [`paths`]: which files the connector may read.
//! * [`token`]: the control API's credential.
//! * [`tools`]: the tool catalog the connector lists.

pub mod abort;
pub mod keys;
pub mod motion;
pub mod occlude;
pub mod paths;
pub mod script;
pub mod spans;
pub mod token;
pub mod tools;
