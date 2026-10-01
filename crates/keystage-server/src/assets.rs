//! Where the built front end comes from. The Tauri app serves its embedded
//! assets (an implementation backed by Tauri's `AssetResolver`); tests and
//! the demo read `dist/` from disk with [`DirAssets`].

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One file of the built front end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub bytes: Cow<'static, [u8]>,
    /// The source's own media type, used only for extensions the server
    /// does not know.
    pub mime: Option<Cow<'static, str>>,
}

impl Asset {
    pub fn new(bytes: impl Into<Cow<'static, [u8]>>) -> Asset {
        Asset {
            bytes: bytes.into(),
            mime: None,
        }
    }
}

/// A source of front-end files.
///
/// `path` is relative to the build root, uses `/`, has no leading slash and
/// has already been checked by [`is_safe_path`]: `overlay.html`,
/// `assets/overlay-BhwiruN9.js`. Return `None` when there is no such file.
/// Called on a blocking thread, so it may read the disk.
pub trait Assets: Send + Sync + 'static {
    fn get(&self, path: &str) -> Option<Asset>;
}

/// True for paths the server will look up: one or more `/`-separated
/// segments of ASCII letters, digits, `.`, `_` and `-`, none empty, none
/// starting with a dot (so no `..` and no hidden files), at most 512 bytes.
/// Anything else (percent escapes included) is a 404 without a lookup.
pub fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && path.split('/').all(|seg| {
            !seg.is_empty()
                && !seg.starts_with('.')
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
}

/// Reads files from a directory on disk (the Vite `dist/` folder).
#[derive(Clone, Debug)]
pub struct DirAssets {
    root: PathBuf,
}

impl DirAssets {
    /// Fails when `root` is not a directory.
    pub fn new(root: impl AsRef<Path>) -> io::Result<DirAssets> {
        let root = root.as_ref().canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is not a folder", root.display()),
            ));
        }
        Ok(DirAssets { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Assets for DirAssets {
    fn get(&self, path: &str) -> Option<Asset> {
        if !is_safe_path(path) {
            return None;
        }
        let mut full = self.root.clone();
        for seg in path.split('/') {
            full.push(seg);
        }
        // Resolve links and refuse anything that leaves the root.
        let real = full.canonicalize().ok()?;
        if !real.starts_with(&self.root) || !real.is_file() {
            return None;
        }
        fs::read(real).ok().map(Asset::new)
    }
}

/// Files held in memory (tests, or a host that has its assets in hand).
#[derive(Clone, Debug, Default)]
pub struct MemoryAssets {
    files: HashMap<String, Vec<u8>>,
}

impl MemoryAssets {
    pub fn new() -> MemoryAssets {
        MemoryAssets::default()
    }

    /// Adds or replaces a file (`path` as in [`Assets::get`]).
    pub fn insert(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> &mut Self {
        self.files.insert(path.into(), bytes.into());
        self
    }
}

impl Assets for MemoryAssets {
    fn get(&self, path: &str) -> Option<Asset> {
        self.files.get(path).map(|b| Asset::new(b.clone()))
    }
}

/// The `Content-Type` for a file. Text types carry `charset=utf-8`; known
/// web types are fixed here so the result never depends on the host's
/// registry; anything else falls back to the source's type, then to a guess
/// from the extension, then to `application/octet-stream`.
pub fn content_type(path: &str, source_mime: Option<&str>) -> Cow<'static, str> {
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let fixed = match ext.as_str() {
        "html" | "htm" => Some("text/html; charset=utf-8"),
        "js" | "mjs" => Some("text/javascript; charset=utf-8"),
        "css" => Some("text/css; charset=utf-8"),
        "json" | "map" => Some("application/json"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "ico" => Some("image/x-icon"),
        "woff2" => Some("font/woff2"),
        "woff" => Some("font/woff"),
        "ttf" => Some("font/ttf"),
        "otf" => Some("font/otf"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        "mp3" => Some("audio/mpeg"),
        "txt" => Some("text/plain; charset=utf-8"),
        "wasm" => Some("application/wasm"),
        _ => None,
    };
    if let Some(t) = fixed {
        return Cow::Borrowed(t);
    }
    if let Some(m) = source_mime.filter(|m| !m.trim().is_empty()) {
        return Cow::Owned(m.to_string());
    }
    match mime_guess::from_ext(&ext).first() {
        Some(m) => Cow::Owned(m.essence_str().to_string()),
        None => Cow::Borrowed("application/octet-stream"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_paths() {
        for ok in [
            "overlay.html",
            "assets/overlay-BhwiruN9.js",
            "assets/rajdhani-latin-500-normal-ByNUCHrt.woff2",
            "sounds/soft_linear/down-1.wav",
        ] {
            assert!(is_safe_path(ok), "{ok}");
        }
        for bad in [
            "",
            "/overlay.html",
            "assets/",
            "assets//x.js",
            "../Cargo.toml",
            "assets/../../etc/passwd",
            "assets/.hidden",
            "assets/%2e%2e/x",
            "assets\\x.js",
            "assets/x y.js",
            "assets/é.js",
            "C:/x.js",
        ] {
            assert!(!is_safe_path(bad), "{bad}");
        }
        assert!(!is_safe_path(&"a".repeat(513)));
    }

    #[test]
    fn content_types() {
        assert_eq!(
            content_type("overlay.html", None),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type("assets/a.JS", Some("application/x-wrong")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type("assets/a.css", None),
            "text/css; charset=utf-8"
        );
        assert_eq!(content_type("assets/a.woff2", None), "font/woff2");
        assert_eq!(content_type("assets/a.woff", None), "font/woff");
        assert_eq!(content_type("sounds/a.wav", None), "audio/wav");
        assert_eq!(content_type("assets/a.svg", None), "image/svg+xml");
        assert_eq!(
            content_type("assets/a.webp", Some("image/webp")),
            "image/webp"
        );
        assert_eq!(content_type("assets/a.gif", None), "image/gif");
        assert_eq!(
            content_type("assets/noext", None),
            "application/octet-stream"
        );
    }

    #[test]
    fn dir_assets_stay_inside_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let dist = dir.path().join("dist");
        fs::create_dir_all(dist.join("assets")).unwrap();
        fs::write(dist.join("overlay.html"), "<!doctype html>").unwrap();
        fs::write(dist.join("assets").join("a.js"), "1").unwrap();
        fs::write(dir.path().join("secret.txt"), "no").unwrap();
        let a = DirAssets::new(&dist).unwrap();
        assert_eq!(
            a.get("overlay.html").unwrap().bytes.as_ref(),
            b"<!doctype html>"
        );
        assert_eq!(a.get("assets/a.js").unwrap().bytes.as_ref(), b"1");
        assert!(a.get("assets").is_none(), "a folder is not a file");
        assert!(a.get("missing.js").is_none());
        assert!(a.get("../secret.txt").is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("secret.txt"), dist.join("link.txt"))
                .unwrap();
            assert!(a.get("link.txt").is_none(), "a link out of the root");
        }
        assert!(DirAssets::new(dir.path().join("secret.txt")).is_err());
        assert!(DirAssets::new(dir.path().join("nope")).is_err());
    }

    #[test]
    fn memory_assets() {
        let mut m = MemoryAssets::new();
        m.insert("overlay.html", "x")
            .insert("assets/a.js", vec![1u8, 2]);
        assert_eq!(m.get("assets/a.js").unwrap().bytes.as_ref(), &[1, 2]);
        assert!(m.get("nope").is_none());
    }
}
