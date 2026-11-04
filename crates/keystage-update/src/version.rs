//! Semantic versions as the release tags write them (`v0.2.0`), compared by
//! semver 2.0.0 precedence (https://semver.org/#spec-item-11). Build
//! metadata (`+...`) is refused rather than ignored: no release of this app
//! carries any, so a tag with it is not one of ours.

use std::cmp::Ordering;
use std::fmt;

/// `MAJOR.MINOR.PATCH` with an optional pre-release (`0.3.0-rc.1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Dot-separated identifiers; empty for a release.
    pub pre: Vec<String>,
}

fn number(s: &str) -> Option<u64> {
    // No leading zeros (semver item 2), digits only, fits in u64.
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

impl Version {
    /// Reads `0.2.0`, `v0.2.0` or `0.3.0-rc.1`. Anything else is `None`.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.strip_prefix('v').unwrap_or(s);
        if s.contains('+') {
            return None;
        }
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let mut parts = core.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(p) => {
                let ids: Vec<String> = p.split('.').map(str::to_string).collect();
                for id in &ids {
                    let ok = !id.is_empty()
                        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                        && !(id.bytes().all(|b| b.is_ascii_digit())
                            && id.len() > 1
                            && id.starts_with('0'));
                    if !ok {
                        return None;
                    }
                }
                ids
            }
        };
        Some(Version {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

fn cmp_pre_id(a: &str, b: &str) -> Ordering {
    let an = a.bytes().all(|c| c.is_ascii_digit());
    let bn = b.bytes().all(|c| c.is_ascii_digit());
    match (an, bn) {
        (true, true) => {
            let x: u64 = a.parse().unwrap_or(u64::MAX);
            let y: u64 = b.parse().unwrap_or(u64::MAX);
            x.cmp(&y)
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                // A pre-release is lower than the release it leads up to.
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(other.pre.iter()) {
                        let o = cmp_pre_id(a, b);
                        if o != Ordering::Equal {
                            return o;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap_or_else(|| panic!("{s} should parse"))
    }

    #[test]
    fn parses_tags_and_plain_versions() {
        assert_eq!(v("v0.2.0"), v("0.2.0"));
        assert_eq!(v("1.20.3").to_string(), "1.20.3");
        assert_eq!(v("0.3.0-rc.1").to_string(), "0.3.0-rc.1");
        assert!(v("0.3.0-rc.1").is_prerelease());
        for bad in [
            "",
            "v",
            "1",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.02.3",
            "1.2.x",
            "v1",
            "1.2.3-",
            "1.2.3-a..b",
            "1.2.3-01",
            "1.2.3+build",
            " 1.2.3",
            "1.2.3 ",
            "V1.2.3",
            "-1.2.3",
            "1.2.3-é",
            "99999999999999999999.0.0",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn orders_by_semver_precedence() {
        // The example chain from semver.org item 11.
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "2.0.0",
        ];
        for w in chain.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
            assert!(v(w[1]) > v(w[0]));
        }
        assert!(v("0.10.0") > v("0.9.9"), "numeric, not text");
        assert_eq!(v("0.2.0").cmp(&v("v0.2.0")), Ordering::Equal);
    }
}
