//! The latest GitHub release, as `GET /repos/{owner}/{repo}/releases/latest`
//! returns it (the real v0.1.0 answer is a test below), and the choice of
//! what to download from it.
//!
//! Only the fields used here are read; everything else in the answer is
//! ignored. The choice fails closed: a draft, a pre-release, a tag that is
//! not a version, a missing asset, or an asset whose download address is not
//! exactly the one this repository's release would have, means no update.

use serde::Deserialize;

use crate::version::Version;
use crate::UpdateError;

/// One release asset.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// The parts of a release this app reads.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// Where releases come from. [`Source::github`] is the only one the app
/// uses; tests point it at a local server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The `releases/latest` API address.
    pub latest_url: String,
    /// Asset addresses must be this, then `<tag>/<file name>`.
    pub download_prefix: String,
}

impl Source {
    pub fn github() -> Source {
        Source {
            latest_url: crate::LATEST_URL.to_string(),
            download_prefix: crate::DOWNLOAD_PREFIX.to_string(),
        }
    }
}

/// Largest installer the app downloads.
pub const MAX_INSTALLER_BYTES: u64 = 256 * 1024 * 1024;
/// Largest `SHA256SUMS.txt` the app downloads.
pub const MAX_SUMS_BYTES: u64 = 64 * 1024;
/// Largest release JSON the app reads.
pub const MAX_RELEASE_JSON_BYTES: u64 = 2 * 1024 * 1024;

/// The installer's file name for a version, as `release.yml` publishes it.
pub fn installer_name(version: &Version) -> String {
    format!("IDEA-Keystage_{version}_x64-setup.exe")
}

/// What to download: the installer and the checksum list of one release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub version: Version,
    pub installer: Asset,
    pub sums: Asset,
}

/// What the latest release means for the running version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The release is this version or older.
    UpToDate { latest: Version },
    /// A newer release with both files in place.
    Newer(Plan),
}

impl Release {
    pub fn from_json(text: &str) -> Result<Release, UpdateError> {
        serde_json::from_str(text).map_err(|_| UpdateError::BadRelease)
    }

    /// Decides what to do with this release when `current` is running.
    pub fn verdict(&self, current: &Version, source: &Source) -> Result<Verdict, UpdateError> {
        if self.draft || self.prerelease {
            return Err(UpdateError::NotARelease);
        }
        let Some(latest) = self
            .tag_name
            .strip_prefix('v')
            .and_then(Version::parse)
            .filter(|v| !v.is_prerelease())
        else {
            return Err(UpdateError::BadTag);
        };
        if latest <= *current {
            return Ok(Verdict::UpToDate { latest });
        }
        let want = |name: &str| -> Result<Asset, UpdateError> {
            let found: Vec<&Asset> = self.assets.iter().filter(|a| a.name == name).collect();
            let [asset] = found.as_slice() else {
                return Err(UpdateError::MissingAsset);
            };
            let expected = format!("{}{}/{}", source.download_prefix, self.tag_name, name);
            if asset.browser_download_url != expected {
                return Err(UpdateError::UnexpectedAddress);
            }
            Ok((*asset).clone())
        };
        let installer = want(&installer_name(&latest))?;
        let sums = want(crate::SUMS_NAME)?;
        if installer.size == 0 || installer.size > MAX_INSTALLER_BYTES {
            return Err(UpdateError::BadSize);
        }
        if sums.size > MAX_SUMS_BYTES {
            return Err(UpdateError::BadSize);
        }
        Ok(Verdict::Newer(Plan {
            version: latest,
            installer,
            sums,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str, names: &[&str]) -> Release {
        let assets: Vec<serde_json::Value> = names
            .iter()
            .map(|n| {
                json!({
                    "name": n,
                    "size": if n.ends_with(".txt") { 203 } else { 4_157_333 },
                    "browser_download_url": format!("https://github.com/pina-hash/idea-keystage/releases/download/{tag}/{n}"),
                    "content_type": "application/x-msdownload"
                })
            })
            .collect();
        let v = json!({ "tag_name": tag, "draft": false, "prerelease": false, "name": "x", "assets": assets, "body": "notes" });
        Release::from_json(&v.to_string()).expect("release")
    }

    fn cur(s: &str) -> Version {
        Version::parse(s).expect("v")
    }

    const ALL: [&str; 3] = [
        "IDEA-Keystage_0.3.0_x64-portable.exe",
        "IDEA-Keystage_0.3.0_x64-setup.exe",
        "SHA256SUMS.txt",
    ];

    #[test]
    fn a_newer_release_picks_the_installer_and_the_sums() {
        let r = release("v0.3.0", &ALL);
        let Verdict::Newer(p) = r.verdict(&cur("0.2.0"), &Source::github()).expect("ok") else {
            panic!("expected an update");
        };
        assert_eq!(p.version.to_string(), "0.3.0");
        assert_eq!(p.installer.name, "IDEA-Keystage_0.3.0_x64-setup.exe");
        assert_eq!(p.sums.name, "SHA256SUMS.txt");
    }

    #[test]
    fn the_same_or_an_older_release_is_up_to_date() {
        let r = release("v0.3.0", &ALL);
        for c in ["0.3.0", "0.4.0", "1.0.0"] {
            assert!(matches!(
                r.verdict(&cur(c), &Source::github()),
                Ok(Verdict::UpToDate { .. })
            ));
        }
        // A pre-release build of the same version is older than the release.
        assert!(matches!(
            r.verdict(&cur("0.3.0-rc.1"), &Source::github()),
            Ok(Verdict::Newer(_))
        ));
    }

    #[test]
    fn anything_doubtful_is_refused() {
        let s = Source::github();
        let c = cur("0.2.0");
        let mut r = release("v0.3.0", &ALL);
        r.draft = true;
        assert_eq!(r.verdict(&c, &s), Err(UpdateError::NotARelease));
        let mut r = release("v0.3.0", &ALL);
        r.prerelease = true;
        assert_eq!(r.verdict(&c, &s), Err(UpdateError::NotARelease));
        for tag in ["v1", "0.3.0", "v0.3", "v0.3.0-rc.1", "latest"] {
            assert_eq!(
                release(tag, &ALL).verdict(&c, &s),
                Err(UpdateError::BadTag),
                "{tag}"
            );
        }
        assert_eq!(
            release("v0.3.0", &["SHA256SUMS.txt"]).verdict(&c, &s),
            Err(UpdateError::MissingAsset)
        );
        assert_eq!(
            release("v0.3.0", &["IDEA-Keystage_0.3.0_x64-setup.exe"]).verdict(&c, &s),
            Err(UpdateError::MissingAsset)
        );
        // An installer for another version is not this release's installer.
        assert_eq!(
            release(
                "v0.3.0",
                &["IDEA-Keystage_0.2.9_x64-setup.exe", "SHA256SUMS.txt"]
            )
            .verdict(&c, &s),
            Err(UpdateError::MissingAsset)
        );
        // Two assets with the same name: which one is meant is unclear.
        let mut r = release("v0.3.0", &ALL);
        let dup = r.assets[1].clone();
        r.assets.push(dup);
        assert_eq!(r.verdict(&c, &s), Err(UpdateError::MissingAsset));
        // Download addresses must be this repository's, for this tag, over https.
        for url in [
            "http://github.com/pina-hash/idea-keystage/releases/download/v0.3.0/IDEA-Keystage_0.3.0_x64-setup.exe",
            "https://github.com/someone/idea-keystage/releases/download/v0.3.0/IDEA-Keystage_0.3.0_x64-setup.exe",
            "https://github.com/pina-hash/idea-keystage/releases/download/v0.2.0/IDEA-Keystage_0.3.0_x64-setup.exe",
            "https://github.com.evil.example/pina-hash/idea-keystage/releases/download/v0.3.0/IDEA-Keystage_0.3.0_x64-setup.exe",
            "https://example.com/IDEA-Keystage_0.3.0_x64-setup.exe",
        ] {
            let mut r = release("v0.3.0", &ALL);
            r.assets[1].browser_download_url = url.into();
            assert_eq!(r.verdict(&c, &s), Err(UpdateError::UnexpectedAddress), "{url}");
        }
        let mut r = release("v0.3.0", &ALL);
        r.assets[1].size = 0;
        assert_eq!(r.verdict(&c, &s), Err(UpdateError::BadSize));
        let mut r = release("v0.3.0", &ALL);
        r.assets[1].size = MAX_INSTALLER_BYTES + 1;
        assert_eq!(r.verdict(&c, &s), Err(UpdateError::BadSize));
        assert_eq!(Release::from_json("{}"), Err(UpdateError::BadRelease));
        assert_eq!(Release::from_json("<html>"), Err(UpdateError::BadRelease));
    }

    #[test]
    fn reads_the_real_v0_1_0_answer_shape() {
        // Trimmed from `gh api repos/pina-hash/idea-keystage/releases/latest`, 2026-11-04.
        let text = r#"{"url":"https://api.github.com/repos/pina-hash/idea-keystage/releases/1","tag_name":"v0.1.0","target_commitish":"main","name":"IDEA Keystage 0.1.0","draft":false,"prerelease":false,"created_at":"2026-10-01T15:30:02Z","published_at":"2026-10-01T16:29:35Z","assets":[{"name":"IDEA-Keystage_0.1.0_x64-portable.exe","size":16525312,"content_type":"application/x-msdownload","browser_download_url":"https://github.com/pina-hash/idea-keystage/releases/download/v0.1.0/IDEA-Keystage_0.1.0_x64-portable.exe"},{"name":"IDEA-Keystage_0.1.0_x64-setup.exe","size":4157333,"content_type":"application/x-msdownload","browser_download_url":"https://github.com/pina-hash/idea-keystage/releases/download/v0.1.0/IDEA-Keystage_0.1.0_x64-setup.exe"},{"name":"SHA256SUMS.txt","size":203,"content_type":"text/plain; charset=utf-8","browser_download_url":"https://github.com/pina-hash/idea-keystage/releases/download/v0.1.0/SHA256SUMS.txt"}],"body":"IDEA Keystage 0.1.0"}"#;
        let r = Release::from_json(text).expect("parse");
        let Verdict::Newer(p) = r.verdict(&cur("0.0.9"), &Source::github()).expect("ok") else {
            panic!("newer");
        };
        assert_eq!(p.installer.size, 4_157_333);
        assert!(matches!(
            r.verdict(&cur("0.1.0"), &Source::github()),
            Ok(Verdict::UpToDate { .. })
        ));
    }
}
