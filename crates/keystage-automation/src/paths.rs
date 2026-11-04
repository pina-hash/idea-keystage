//! Which files the connector may read (`recording_info`,
//! `recording_read_chunk`): only files inside OBS's recording folder or the
//! Session Log folder (docs/ARCHITECTURE.md section 14). The path is
//! checked as text first (absolute, no `.` or `..`, no device or stream
//! syntax), then resolved with every link followed and checked again
//! against the resolved roots, so a link inside a root cannot reach out.

use std::path::{Component, Path, PathBuf};

/// Largest chunk one read returns.
pub const MAX_CHUNK: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("give the full path of a file")]
    NotAbsolute,
    #[error("the path may not contain . or .. parts")]
    Traversal,
    #[error("the path may not name a device or a stream")]
    Device,
    #[error("only files in the OBS recording folder or the session log folder can be read")]
    Outside,
    #[error("there is no such file")]
    Missing,
    #[error("that is a folder, not a file")]
    NotAFile,
}

fn lexical(path: &str) -> Result<PathBuf, PathError> {
    if path.is_empty() || path.contains('\0') {
        return Err(PathError::NotAbsolute);
    }
    // \\?\, \\.\ and UNC device paths; a colon past the drive letter is an
    // alternate data stream.
    if path.starts_with(r"\\?\")
        || path.starts_with(r"\\.\")
        || path.starts_with("//?/")
        || path.starts_with("//./")
    {
        return Err(PathError::Device);
    }
    let colons = path.match_indices(':').map(|(i, _)| i).collect::<Vec<_>>();
    if colons.iter().any(|&i| i != 1) {
        return Err(PathError::Device);
    }
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err(PathError::NotAbsolute);
    }
    for c in p.components() {
        match c {
            Component::CurDir | Component::ParentDir => return Err(PathError::Traversal),
            _ => {}
        }
    }
    // Also refuse ".." written with the other slash on this platform.
    if path.split(['/', '\\']).any(|s| s == ".." || s == ".") {
        return Err(PathError::Traversal);
    }
    Ok(p)
}

/// The resolved file, if `path` is a file inside one of `roots`.
pub fn allowed(path: &str, roots: &[PathBuf]) -> Result<PathBuf, PathError> {
    let p = lexical(path)?;
    let real = std::fs::canonicalize(&p).map_err(|_| PathError::Missing)?;
    let inside = roots
        .iter()
        .filter_map(|r| std::fs::canonicalize(r).ok())
        .any(|r| real.starts_with(&r) && real != r);
    if !inside {
        return Err(PathError::Outside);
    }
    if !real.is_file() {
        return Err(PathError::NotAFile);
    }
    Ok(real)
}

/// The byte range to read: `offset` must be inside the file (or at its
/// end, giving nothing), `length` is cut to [`MAX_CHUNK`] and the end.
pub fn chunk_range(size: u64, offset: u64, length: u64) -> Option<(u64, u64)> {
    if offset > size {
        return None;
    }
    let len = length.min(MAX_CHUNK).min(size - offset);
    Some((offset, len))
}

/// For tests: is `p` a plain file name with no folder part.
pub fn is_plain_name(p: &Path) -> bool {
    p.components().count() == 1 && matches!(p.components().next(), Some(Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn only_files_inside_the_roots() {
        let d = tempfile::tempdir().expect("d");
        let rec = d.path().join("Videos");
        let other = d.path().join("Private");
        fs::create_dir_all(&rec).expect("mk");
        fs::create_dir_all(&other).expect("mk");
        fs::write(rec.join("take 1.mp4"), b"video").expect("w");
        fs::write(other.join("secret.txt"), b"no").expect("w");
        let roots = vec![rec.clone()];
        let s = |p: &Path| p.to_string_lossy().to_string();
        assert!(allowed(&s(&rec.join("take 1.mp4")), &roots).is_ok());
        assert_eq!(
            allowed(&s(&other.join("secret.txt")), &roots),
            Err(PathError::Outside)
        );
        let up = format!("{}/../Private/secret.txt", s(&rec));
        assert_eq!(allowed(&up, &roots), Err(PathError::Traversal));
        let dot = format!("{}/./take 1.mp4", s(&rec));
        assert_eq!(allowed(&dot, &roots), Err(PathError::Traversal));
        assert_eq!(allowed("take 1.mp4", &roots), Err(PathError::NotAbsolute));
        assert_eq!(allowed("", &roots), Err(PathError::NotAbsolute));
        assert_eq!(
            allowed(&s(&rec.join("none.mp4")), &roots),
            Err(PathError::Missing)
        );
        assert_eq!(
            allowed(&s(&rec), &roots),
            Err(PathError::Outside),
            "the root itself is not a file"
        );
        assert_eq!(allowed(r"\\?\C:\x.mp4", &roots), Err(PathError::Device));
        assert_eq!(
            allowed(r"\\.\PhysicalDrive0", &roots),
            Err(PathError::Device)
        );
        assert_eq!(
            allowed(r"C:\Videos\a.mp4:secret", &roots),
            Err(PathError::Device)
        );
        assert_eq!(
            allowed(&s(&rec.join("take 1.mp4")), &[]),
            Err(PathError::Outside)
        );
        fs::create_dir_all(rec.join("sub")).expect("mk");
        assert_eq!(
            allowed(&s(&rec.join("sub")), &roots),
            Err(PathError::NotAFile)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_a_root_cannot_reach_out() {
        let d = tempfile::tempdir().expect("d");
        let rec = d.path().join("Videos");
        fs::create_dir_all(&rec).expect("mk");
        fs::write(d.path().join("outside.txt"), b"no").expect("w");
        std::os::unix::fs::symlink(d.path().join("outside.txt"), rec.join("link.mp4")).expect("ln");
        let p = rec.join("link.mp4").to_string_lossy().to_string();
        assert_eq!(allowed(&p, &[rec]), Err(PathError::Outside));
    }

    #[test]
    fn chunks_stay_inside_the_file_and_the_limit() {
        assert_eq!(chunk_range(100, 0, 10), Some((0, 10)));
        assert_eq!(chunk_range(100, 95, 10), Some((95, 5)));
        assert_eq!(chunk_range(100, 100, 10), Some((100, 0)));
        assert_eq!(chunk_range(100, 101, 10), None);
        assert_eq!(chunk_range(u64::MAX, 0, u64::MAX), Some((0, MAX_CHUNK)));
        assert!(is_plain_name(Path::new("a.json")) && !is_plain_name(Path::new("x/a.json")));
    }
}
