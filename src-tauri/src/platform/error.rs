//! Errors from the platform layer.
//!
//! Messages name the failing call and the operating system's own error text.
//! They never contain key names, typed text, window titles or process names,
//! so they are safe to log (ARCHITECTURE.md section 8, rule 5).

use std::io;

/// Everything that can go wrong in the platform layer.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// The feature does not exist on this operating system (the non-Windows
    /// stub returns this for DPAPI, for example).
    #[error("not available on this platform")]
    Unsupported,

    /// A Windows call failed. `code` is the HRESULT (or the Win32 error
    /// converted to one), `message` the system's text for it.
    #[error("{call} failed (0x{code:08X}): {message}")]
    Os {
        call: &'static str,
        code: u32,
        message: String,
    },

    /// A second instance of a single-instance service was requested.
    #[error("the {0} is already running")]
    AlreadyRunning(&'static str),

    /// A worker thread did not report back in time.
    #[error("the {0} thread did not start in time")]
    StartTimeout(&'static str),

    /// A worker thread ended before it reported that it was ready.
    #[error("the {0} thread stopped before it was ready")]
    ThreadExited(&'static str),

    /// The operating system refused to create a thread.
    #[error("could not start the {name} thread: {source}")]
    Spawn {
        name: &'static str,
        #[source]
        source: io::Error,
    },

    /// A window call was made from a thread that does not own the window.
    #[error("{0} must run on the thread that created the window")]
    WrongThread(&'static str),

    /// The caller passed something the function cannot use.
    #[error("{0}")]
    InvalidInput(&'static str),

    /// Protected data could not be decoded or decrypted, or decrypted to
    /// something that is not a valid secret.
    #[error("the protected data could not be read")]
    InvalidData,

    /// A service was used after it was shut down.
    #[error("the {0} is not running")]
    NotRunning(&'static str),
}

#[cfg(windows)]
impl PlatformError {
    /// Wraps a `windows` crate error with the name of the call that failed.
    pub(crate) fn os(call: &'static str, err: &::windows::core::Error) -> PlatformError {
        PlatformError::Os {
            call,
            // HRESULT bits, printed as the familiar 0x8007xxxx form.
            code: err.code().0 as u32,
            message: err.message(),
        }
    }

    /// Builds an error from the calling thread's last Win32 error.
    pub(crate) fn last_os_error(call: &'static str) -> PlatformError {
        PlatformError::os(call, &::windows::core::Error::from_thread())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_plain_and_name_the_call() {
        let e = PlatformError::Os {
            call: "SetWindowsHookExW",
            code: 0x8007_0005,
            message: "Access is denied.".into(),
        };
        assert_eq!(
            e.to_string(),
            "SetWindowsHookExW failed (0x80070005): Access is denied."
        );
        assert_eq!(
            PlatformError::AlreadyRunning("input hook").to_string(),
            "the input hook is already running"
        );
        assert_eq!(
            PlatformError::StartTimeout("watcher").to_string(),
            "the watcher thread did not start in time"
        );
        assert_eq!(
            PlatformError::Unsupported.to_string(),
            "not available on this platform"
        );
        let spawn = PlatformError::Spawn {
            name: "hook",
            source: io::Error::other("no threads left"),
        };
        assert_eq!(
            spawn.to_string(),
            "could not start the hook thread: no threads left"
        );
        assert!(std::error::Error::source(&spawn).is_some());
    }
}
