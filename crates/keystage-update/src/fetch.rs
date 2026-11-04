//! The only network code in IDEA Keystage (ARCHITECTURE.md section 8, rule
//! 7): HTTPS GET requests for the latest release and its two files.
//!
//! Every request carries the same fixed headers ([`USER_AGENT`], the GitHub
//! `Accept` and API version) and nothing else: no identifiers, no cookies
//! (ureq's cookie store is not compiled in), no telemetry. Only `https://`
//! addresses are fetched, redirects included (`https_only`). The installer
//! is streamed to a `.part` file while it is hashed, checked against
//! `SHA256SUMS.txt` and its listed size, and renamed into place only when
//! both match; anything else deletes the file.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::release::{
    Plan, Release, Source, MAX_INSTALLER_BYTES, MAX_RELEASE_JSON_BYTES, MAX_SUMS_BYTES,
};
use crate::sums::{self, Digest256};
use crate::UpdateError;

/// The user agent GitHub requires. Names the app only, not its version or
/// the computer.
pub const USER_AGENT: &str = "IDEA-Keystage-updater";

/// An HTTP client for releases.
pub struct Client {
    agent: ureq::Agent,
}

fn map_err(e: ureq::Error) -> UpdateError {
    match e {
        ureq::Error::StatusCode(404) => UpdateError::NoRelease,
        ureq::Error::StatusCode(403 | 429) => UpdateError::RateLimited,
        ureq::Error::StatusCode(_) => UpdateError::Server,
        ureq::Error::BodyExceedsLimit(_) => UpdateError::BadSize,
        ureq::Error::RequireHttpsOnly(_) => UpdateError::UnexpectedAddress,
        _ => UpdateError::Network,
    }
}

impl Client {
    /// The app's client: HTTPS only, the platform proxy settings from the
    /// environment, at most 30 s to connect.
    pub fn new() -> Client {
        Client::build(true, true)
    }

    fn build(https_only: bool, env_proxy: bool) -> Client {
        let mut b = ureq::Agent::config_builder()
            .https_only(https_only)
            .user_agent(USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_global(Some(Duration::from_secs(15 * 60)))
            .max_redirects(5)
            .save_redirect_history(false)
            .http_status_as_error(true);
        if !env_proxy {
            b = b.proxy(None);
        }
        Client {
            agent: b.build().new_agent(),
        }
    }

    /// A plain-HTTP client for tests against a local server.
    #[cfg(test)]
    fn local() -> Client {
        Client::build(false, false)
    }

    fn get(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, UpdateError> {
        self.agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(map_err)
    }

    /// `GET releases/latest`.
    pub fn latest(&self, source: &Source) -> Result<Release, UpdateError> {
        let mut r = self.get(&source.latest_url)?;
        let text = r
            .body_mut()
            .with_config()
            .limit(MAX_RELEASE_JSON_BYTES)
            .read_to_string()
            .map_err(map_err)?;
        Release::from_json(&text)
    }

    /// Downloads the plan's installer into `dir`, verified. Returns its path.
    pub fn download(&self, plan: &Plan, dir: &Path) -> Result<PathBuf, UpdateError> {
        let mut r = self.get(&plan.sums.browser_download_url)?;
        let sums_text = r
            .body_mut()
            .with_config()
            .limit(MAX_SUMS_BYTES)
            .read_to_string()
            .map_err(map_err)?;
        let expected = sums::expected_digest(&sums_text, &plan.installer.name)?;

        fs::create_dir_all(dir).map_err(|_| UpdateError::Disk)?;
        let final_path = crate::install::installer_path(dir, &plan.version);
        let part = final_path.with_extension("exe.part");
        let result = (|| {
            let mut r = self.get(&plan.installer.browser_download_url)?;
            let reader = r
                .body_mut()
                .with_config()
                .limit(MAX_INSTALLER_BYTES)
                .reader();
            let mut file = File::create(&part).map_err(|_| UpdateError::Disk)?;
            let (digest, len) = copy_hashed(reader, &mut file)?;
            file.sync_all().map_err(|_| UpdateError::Disk)?;
            drop(file);
            if len != plan.installer.size {
                return Err(UpdateError::BadSize);
            }
            if digest != expected {
                return Err(UpdateError::ChecksumMismatch);
            }
            let _ = fs::remove_file(&final_path);
            fs::rename(&part, &final_path).map_err(|_| UpdateError::Disk)?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(&part);
            return Err(e);
        }
        // Read back from disk once more: what installs is what was checked.
        verify_file(&final_path, &expected).inspect_err(|_| {
            let _ = fs::remove_file(&final_path);
        })?;
        Ok(final_path)
    }
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

fn copy_hashed(mut r: impl Read, w: &mut impl Write) -> Result<(Digest256, u64), UpdateError> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                // ureq reports an exceeded limit as an io error wrapping its own.
                let text = e.to_string();
                return Err(if text.contains("limit") {
                    UpdateError::BadSize
                } else {
                    UpdateError::Network
                });
            }
        };
        h.update(&buf[..n]);
        w.write_all(&buf[..n]).map_err(|_| UpdateError::Disk)?;
        total += n as u64;
    }
    Ok((h.finalize().into(), total))
}

/// Hashes the file again and compares it with `expected`.
pub fn verify_file(path: &Path, expected: &Digest256) -> Result<(), UpdateError> {
    let f = File::open(path).map_err(|_| UpdateError::Disk)?;
    let d = sums::digest_reader(f).map_err(|_| UpdateError::Disk)?;
    if &d == expected {
        Ok(())
    } else {
        Err(UpdateError::ChecksumMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::Verdict;
    use crate::version::Version;
    use std::collections::HashMap;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// A tiny HTTP/1.1 server: path to (status, body). Records each
    /// request's headers so the test can check what was sent.
    struct Server {
        base: String,
        seen: Arc<Mutex<Vec<Vec<String>>>>,
    }

    fn serve(routes: HashMap<String, (u16, Vec<u8>)>) -> Server {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", l.local_addr().expect("addr"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        thread::spawn(move || {
            for stream in l.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut reader = io::BufReader::new(s.try_clone().expect("clone"));
                let mut lines = Vec::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let t = line.trim_end().to_string();
                    if t.is_empty() {
                        break;
                    }
                    lines.push(t);
                }
                let path = lines
                    .first()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("/")
                    .to_string();
                seen2.lock().expect("lock").push(lines);
                let (status, body) = routes.get(&path).cloned().unwrap_or((404, b"{}".to_vec()));
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(&body);
            }
        });
        Server { base, seen }
    }

    fn setup(installer: &[u8], listed: &[u8], size: Option<u64>) -> (Server, Source) {
        let name = "IDEA-Keystage_0.3.0_x64-setup.exe";
        let sums_text = format!("{}  {name}\n", sums::to_hex(&sums::digest(listed)));
        let mut routes = HashMap::new();
        let prefix = "/dl/".to_string();
        routes.insert(format!("{prefix}v0.3.0/{name}"), (200, installer.to_vec()));
        routes.insert(
            format!("{prefix}v0.3.0/SHA256SUMS.txt"),
            (200, sums_text.into_bytes()),
        );
        let srv = serve(routes.clone());
        let source = Source {
            latest_url: format!("{}/latest", srv.base),
            download_prefix: format!("{}{prefix}", srv.base),
        };
        let release = serde_json::json!({
            "tag_name": "v0.3.0",
            "assets": [
                {"name": name, "size": size.unwrap_or(installer.len() as u64), "browser_download_url": format!("{}v0.3.0/{name}", source.download_prefix)},
                {"name": "SHA256SUMS.txt", "size": 100, "browser_download_url": format!("{}v0.3.0/SHA256SUMS.txt", source.download_prefix)}
            ]
        });
        // A second server answers `latest` with the addresses of the first.
        let mut r2 = routes;
        r2.insert("/latest".into(), (200, release.to_string().into_bytes()));
        let srv2 = serve(r2);
        let source = Source {
            latest_url: format!("{}/latest", srv2.base),
            download_prefix: source.download_prefix,
        };
        (srv, source)
    }

    fn plan(c: &Client, source: &Source) -> Plan {
        let r = c.latest(source).expect("latest");
        match r
            .verdict(&Version::parse("0.2.0").expect("v"), source)
            .expect("v")
        {
            Verdict::Newer(p) => p,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn downloads_and_verifies_the_installer() {
        let body = b"MZ fake installer bytes".repeat(5000);
        let (srv, source) = setup(&body, &body, None);
        let c = Client::local();
        let p = plan(&c, &source);
        let d = tempfile::tempdir().expect("d");
        let path = c.download(&p, d.path()).expect("download");
        assert_eq!(fs::read(&path).expect("read"), body);
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("IDEA-Keystage_0.3.0_x64-setup.exe")
        );
        let leftovers: Vec<_> = fs::read_dir(d.path())
            .expect("ls")
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "no .part file left: {leftovers:?}");
        // What was sent: the fixed headers, no cookies, no identifiers.
        let seen = srv.seen.lock().expect("lock").clone();
        assert_eq!(seen.len(), 2, "sums then installer");
        for req in &seen {
            let names: Vec<String> = req[1..]
                .iter()
                .map(|h| h.split(':').next().unwrap_or("").to_ascii_lowercase())
                .collect();
            for n in &names {
                assert!(
                    [
                        "host",
                        "user-agent",
                        "accept",
                        "x-github-api-version",
                        "accept-encoding"
                    ]
                    .contains(&n.as_str()),
                    "unexpected header {n}"
                );
            }
            assert!(
                req.iter().any(|h| h == &format!("user-agent: {USER_AGENT}")
                    || h == &format!("User-Agent: {USER_AGENT}")),
                "{req:?}"
            );
        }
    }

    #[test]
    fn a_wrong_checksum_or_size_leaves_nothing_behind() {
        let body = b"MZ real".to_vec();
        let (_srv, source) = setup(&body, b"MZ something else", None);
        let c = Client::local();
        let p = plan(&c, &source);
        let d = tempfile::tempdir().expect("d");
        assert_eq!(c.download(&p, d.path()), Err(UpdateError::ChecksumMismatch));
        assert_eq!(fs::read_dir(d.path()).expect("ls").count(), 0);

        let (_srv, source) = setup(&body, &body, Some(body.len() as u64 + 1));
        let p = plan(&c, &source);
        assert_eq!(c.download(&p, d.path()), Err(UpdateError::BadSize));
        assert_eq!(fs::read_dir(d.path()).expect("ls").count(), 0);
    }

    #[test]
    fn http_answers_map_to_plain_errors() {
        let srv = serve(HashMap::from([
            ("/gone".to_string(), (404, b"{}".to_vec())),
            ("/limit".to_string(), (403, b"{}".to_vec())),
            ("/broken".to_string(), (500, b"{}".to_vec())),
            ("/html".to_string(), (200, b"<html></html>".to_vec())),
        ]));
        let c = Client::local();
        let at = |p: &str| Source {
            latest_url: format!("{}{p}", srv.base),
            download_prefix: String::new(),
        };
        assert_eq!(c.latest(&at("/gone")), Err(UpdateError::NoRelease));
        assert_eq!(c.latest(&at("/limit")), Err(UpdateError::RateLimited));
        assert_eq!(c.latest(&at("/broken")), Err(UpdateError::Server));
        assert_eq!(c.latest(&at("/html")), Err(UpdateError::BadRelease));
        // The app's own client refuses plain http before connecting.
        assert_eq!(
            Client::new().latest(&at("/gone")),
            Err(UpdateError::UnexpectedAddress)
        );
    }

    #[test]
    fn verify_file_compares_the_bytes_on_disk() {
        let d = tempfile::tempdir().expect("d");
        let p = d.path().join("x.exe");
        fs::write(&p, b"abc").expect("w");
        assert_eq!(verify_file(&p, &sums::digest(b"abc")), Ok(()));
        assert_eq!(
            verify_file(&p, &sums::digest(b"abd")),
            Err(UpdateError::ChecksumMismatch)
        );
        assert_eq!(
            verify_file(&d.path().join("none"), &sums::digest(b"")),
            Err(UpdateError::Disk)
        );
    }
}
