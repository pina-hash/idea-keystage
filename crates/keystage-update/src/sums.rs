//! `SHA256SUMS.txt` as `release.yml` writes it (`sha256sum -b`, with the
//! binary marker turned into two spaces): one `<64 hex digits>  <file name>`
//! line per file. The reader also takes the `*name` binary form. It refuses
//! anything it is not sure about: a malformed line, a name listed twice, or
//! no line for the file asked about.

use std::io::{self, Read};

use sha2::{Digest, Sha256};

use crate::UpdateError;

/// A SHA-256 digest.
pub type Digest256 = [u8; 32];

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Reads 64 hex digits.
pub fn parse_hex(s: &str) -> Option<Digest256> {
    let b = s.as_bytes();
    if b.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in b.chunks_exact(2).enumerate() {
        out[i] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
    }
    Some(out)
}

/// Lowercase hex.
pub fn to_hex(d: &Digest256) -> String {
    let mut s = String::with_capacity(64);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The digest listed for `name`.
pub fn expected_digest(sums: &str, name: &str) -> Result<Digest256, UpdateError> {
    let text = sums.strip_prefix('\u{feff}').unwrap_or(sums);
    let mut found: Option<Digest256> = None;
    for raw in text.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            continue;
        }
        let (hash, rest) = line.split_at_checked(64).ok_or(UpdateError::BadChecksums)?;
        let digest = parse_hex(hash).ok_or(UpdateError::BadChecksums)?;
        let file = rest
            .strip_prefix("  ")
            .or_else(|| rest.strip_prefix(" *"))
            .ok_or(UpdateError::BadChecksums)?;
        if file.is_empty() || file.contains('/') || file.contains('\\') {
            return Err(UpdateError::BadChecksums);
        }
        if file == name {
            if found.is_some() {
                return Err(UpdateError::BadChecksums);
            }
            found = Some(digest);
        }
    }
    found.ok_or(UpdateError::NotInChecksums)
}

/// The SHA-256 of everything `r` yields.
pub fn digest_reader(mut r: impl Read) -> io::Result<Digest256> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().into())
}

/// The SHA-256 of `bytes`.
pub fn digest(bytes: &[u8]) -> Digest256 {
    Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn digests_match_the_published_test_vector() {
        assert_eq!(to_hex(&digest(b"")), EMPTY);
        assert_eq!(
            to_hex(&digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            digest_reader(&b"abc"[..]).expect("read"),
            digest(b"abc"),
            "streaming and one-shot agree"
        );
        assert_eq!(parse_hex(EMPTY).map(|d| to_hex(&d)).as_deref(), Some(EMPTY));
        assert_eq!(
            parse_hex(&EMPTY.to_uppercase())
                .map(|d| to_hex(&d))
                .as_deref(),
            Some(EMPTY)
        );
        assert_eq!(parse_hex(&EMPTY[..63]), None);
        assert_eq!(parse_hex(&format!("{}g", &EMPTY[..63])), None);
    }

    #[test]
    fn reads_the_release_workflow_format() {
        // The shape `release.yml` writes (v0.1.0's file, hashes replaced).
        let a = "1".repeat(64);
        let b = "2".repeat(64);
        let text = format!(
            "{a}  IDEA-Keystage_0.1.0_x64-portable.exe\n{b}  IDEA-Keystage_0.1.0_x64-setup.exe\n"
        );
        let d = expected_digest(&text, "IDEA-Keystage_0.1.0_x64-setup.exe").expect("found");
        assert_eq!(to_hex(&d), b);
        // CRLF, a BOM and the `*name` binary marker are read too.
        let text = format!("\u{feff}{a} *one.exe\r\n\r\n{b}  two.exe\r\n");
        assert_eq!(to_hex(&expected_digest(&text, "one.exe").expect("one")), a);
        assert_eq!(to_hex(&expected_digest(&text, "two.exe").expect("two")), b);
    }

    #[test]
    fn refuses_what_it_is_not_sure_about() {
        let a = "a".repeat(64);
        assert_eq!(
            expected_digest(&format!("{a}  other.exe\n"), "setup.exe"),
            Err(UpdateError::NotInChecksums)
        );
        assert_eq!(
            expected_digest("", "setup.exe"),
            Err(UpdateError::NotInChecksums)
        );
        for bad in [
            format!("{a} setup.exe"),
            format!("{}  setup.exe", &a[..63]),
            format!("{a}  "),
            format!("{a}  dir/setup.exe"),
            format!("{a}\tsetup.exe"),
            "not a checksum line".to_string(),
            format!("{a}  setup.exe\n{}  setup.exe", "b".repeat(64)),
            format!("{a}  setup.exe\n{a}  setup.exe"),
        ] {
            assert_eq!(
                expected_digest(&bad, "setup.exe"),
                Err(UpdateError::BadChecksums),
                "{bad:?}"
            );
        }
    }
}
