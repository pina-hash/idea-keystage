//! keystage-update: IDEA Keystage's automatic updates (docs/ARCHITECTURE.md
//! section 13, docs/DECISIONS.md item 9).
//!
//! * [`version`] compares release tags by semver precedence.
//! * [`release`] reads the latest GitHub release and picks the installer
//!   and `SHA256SUMS.txt`, refusing anything doubtful.
//! * [`sums`] reads `SHA256SUMS.txt` and hashes files.
//! * [`fetch`] is the HTTPS client (the app's only outbound requests).
//! * [`machine`] decides when to check and when an update may install.
//! * [`install`] holds the installer's switches and the marker the next run
//!   reads.
//!
//! Starting the installer is Windows code and lives in the app shell.
//! Nothing here logs.

pub mod fetch;
pub mod install;
pub mod machine;
pub mod release;
pub mod sums;
pub mod version;

pub use machine::{Env, Phase, Tick, Updater};
pub use release::{Plan, Release, Source, Verdict};
pub use version::Version;

/// The repository releases come from.
pub const REPOSITORY: &str = "pina-hash/idea-keystage";
/// `GET /repos/{owner}/{repo}/releases/latest`.
pub const LATEST_URL: &str = "https://api.github.com/repos/pina-hash/idea-keystage/releases/latest";
/// Every asset address starts with this, then `<tag>/<file name>`.
pub const DOWNLOAD_PREFIX: &str = "https://github.com/pina-hash/idea-keystage/releases/download/";
/// The checksum list every release carries.
pub const SUMS_NAME: &str = "SHA256SUMS.txt";
/// The releases page, for a copy that does not update itself.
pub const RELEASES_PAGE: &str = "https://github.com/pina-hash/idea-keystage/releases/latest";

/// Why an update did not happen. [`UpdateError::message`] is the sentence
/// the user sees; none of them names a path, a host or an error code.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("network")]
    Network,
    #[error("rate limited")]
    RateLimited,
    #[error("server")]
    Server,
    #[error("no release")]
    NoRelease,
    #[error("bad release")]
    BadRelease,
    #[error("not a release")]
    NotARelease,
    #[error("bad tag")]
    BadTag,
    #[error("missing asset")]
    MissingAsset,
    #[error("unexpected address")]
    UnexpectedAddress,
    #[error("bad size")]
    BadSize,
    #[error("bad checksums")]
    BadChecksums,
    #[error("not in checksums")]
    NotInChecksums,
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("disk")]
    Disk,
    #[error("launch failed")]
    LaunchFailed,
    #[error("did not finish")]
    DidNotFinish(String),
}

impl UpdateError {
    /// A short, fixed reason for the log (privacy rule 5: lifecycle only).
    pub fn reason(&self) -> &'static str {
        match self {
            UpdateError::Network => "network",
            UpdateError::RateLimited => "rate_limited",
            UpdateError::Server => "server",
            UpdateError::NoRelease => "no_release",
            UpdateError::BadRelease => "bad_release",
            UpdateError::NotARelease => "not_a_release",
            UpdateError::BadTag => "bad_tag",
            UpdateError::MissingAsset => "missing_asset",
            UpdateError::UnexpectedAddress => "unexpected_address",
            UpdateError::BadSize => "bad_size",
            UpdateError::BadChecksums => "bad_checksums",
            UpdateError::NotInChecksums => "not_in_checksums",
            UpdateError::ChecksumMismatch => "checksum_mismatch",
            UpdateError::Disk => "disk",
            UpdateError::LaunchFailed => "launch_failed",
            UpdateError::DidNotFinish(_) => "did_not_finish",
        }
    }

    /// The sentence the user sees.
    pub fn message(&self) -> String {
        let s = match self {
            UpdateError::Network => "The update check could not reach GitHub. It tries again in 30 minutes.",
            UpdateError::RateLimited => "GitHub asked for fewer update checks from this network. It tries again in 30 minutes.",
            UpdateError::Server => "GitHub did not answer the update check. It tries again in 30 minutes.",
            UpdateError::NoRelease => "No release of IDEA Keystage was found.",
            UpdateError::BadRelease => "The release information could not be read, so nothing was installed.",
            UpdateError::NotARelease => "The newest release is not a final release, so nothing was installed.",
            UpdateError::BadTag => "The newest release has a name that is not a version number, so nothing was installed.",
            UpdateError::MissingAsset => "The newest release has no installer, so nothing was installed.",
            UpdateError::UnexpectedAddress => "The newest release points somewhere unexpected, so nothing was installed.",
            UpdateError::BadSize => "The download was not the size the release lists, so it was not installed.",
            UpdateError::BadChecksums => "The release checksum list could not be read, so nothing was installed.",
            UpdateError::NotInChecksums => "The release checksum list does not list the installer, so nothing was installed.",
            UpdateError::ChecksumMismatch => "The download did not match its checksum, so it was not installed.",
            UpdateError::Disk => "The update could not be saved to disk, so nothing was installed.",
            UpdateError::LaunchFailed => "The installer could not be started. IDEA Keystage keeps running this version.",
            UpdateError::DidNotFinish(v) => {
                return format!("The update to {v} did not finish. IDEA Keystage tries again later.");
            }
        };
        s.to_string()
    }
}
