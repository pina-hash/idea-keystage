//! The platform-neutral half of secret storage: base64 framing, input and
//! output validation, and wiping plaintext copies. The Windows half
//! (`platform/windows/dpapi.rs`) supplies `CryptProtectData` and
//! `CryptUnprotectData`; other platforms have no protector, so nothing is
//! ever stored there in plain text.
//!
//! Microsoft notes that tampered DPAPI data "may in some cases succeed with
//! corrupted output" (CryptUnprotectData), so decrypted bytes are checked:
//! they must be valid UTF-8 of a sane length.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

use super::PlatformError;

/// The longest secret accepted, in bytes. The OBS password is the only
/// secret; obs-websocket generates 16 characters.
pub const MAX_SECRET_BYTES: usize = 1024;

/// The longest stored blob accepted, in base64 characters. DPAPI adds a few
/// hundred bytes of header and MAC to the plaintext.
pub const MAX_BLOB_CHARS: usize = 8 * 1024;

/// Overwrites a buffer with zeros in a way the optimizer keeps.
pub fn wipe(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        // SAFETY: `b` is a valid, aligned, exclusive reference.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// Protects `plain` with `protect` and returns the blob as base64.
pub fn protect_with(
    plain: &str,
    protect: impl FnOnce(&[u8]) -> Result<Vec<u8>, PlatformError>,
) -> Result<String, PlatformError> {
    if plain.is_empty() {
        return Err(PlatformError::InvalidInput("the secret is empty"));
    }
    if plain.len() > MAX_SECRET_BYTES {
        return Err(PlatformError::InvalidInput("the secret is too long"));
    }
    let blob = protect(plain.as_bytes())?;
    if blob.is_empty() {
        return Err(PlatformError::InvalidData);
    }
    Ok(STANDARD.encode(blob))
}

/// Decodes a base64 blob, unprotects it with `unprotect`, and validates the
/// result. The intermediate plaintext buffer is wiped.
pub fn unprotect_with(
    blob_b64: &str,
    unprotect: impl FnOnce(&[u8]) -> Result<Vec<u8>, PlatformError>,
) -> Result<String, PlatformError> {
    let trimmed = blob_b64.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_BLOB_CHARS {
        return Err(PlatformError::InvalidData);
    }
    let blob = STANDARD
        .decode(trimmed)
        .map_err(|_| PlatformError::InvalidData)?;
    if blob.is_empty() {
        return Err(PlatformError::InvalidData);
    }
    let mut plain = unprotect(&blob)?;
    let result = validate(&plain);
    wipe(&mut plain);
    result
}

fn validate(plain: &[u8]) -> Result<String, PlatformError> {
    if plain.is_empty() || plain.len() > MAX_SECRET_BYTES {
        return Err(PlatformError::InvalidData);
    }
    let s = std::str::from_utf8(plain).map_err(|_| PlatformError::InvalidData)?;
    if s.chars().any(|c| c == '\0') {
        return Err(PlatformError::InvalidData);
    }
    Ok(s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A reversible stand-in for DPAPI: a header plus the bytes XORed.
    fn fake_protect(p: &[u8]) -> Result<Vec<u8>, PlatformError> {
        let mut v = b"KS1".to_vec();
        v.extend(p.iter().map(|b| b ^ 0x5A));
        Ok(v)
    }

    fn fake_unprotect(b: &[u8]) -> Result<Vec<u8>, PlatformError> {
        let body = b.strip_prefix(b"KS1").ok_or(PlatformError::InvalidData)?;
        Ok(body.iter().map(|b| b ^ 0x5A).collect())
    }

    #[test]
    fn round_trip() {
        let blob = protect_with("s3cret pässword", fake_protect).expect("protect");
        assert!(!blob.contains("s3cret"));
        assert_eq!(
            unprotect_with(&blob, fake_unprotect).expect("unprotect"),
            "s3cret pässword"
        );
        // Surrounding whitespace in the stored value is tolerated.
        assert_eq!(
            unprotect_with(&format!("  {blob}\n"), fake_unprotect).expect("unprotect"),
            "s3cret pässword"
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(
            protect_with("", fake_protect),
            Err(PlatformError::InvalidInput(_))
        ));
        let long = "x".repeat(MAX_SECRET_BYTES + 1);
        assert!(matches!(
            protect_with(&long, fake_protect),
            Err(PlatformError::InvalidInput(_))
        ));
        assert!(matches!(
            protect_with("ok", |_| Ok(Vec::new())),
            Err(PlatformError::InvalidData)
        ));
        assert!(matches!(
            protect_with("ok", |_| Err(PlatformError::Unsupported)),
            Err(PlatformError::Unsupported)
        ));
    }

    #[test]
    fn rejects_bad_blobs_and_bad_plaintext() {
        for bad in ["", "   ", "not base64!", "AAAA*"] {
            assert!(
                matches!(
                    unprotect_with(bad, fake_unprotect),
                    Err(PlatformError::InvalidData)
                ),
                "{bad:?}"
            );
        }
        let too_long = "A".repeat(MAX_BLOB_CHARS + 4);
        assert!(matches!(
            unprotect_with(&too_long, fake_unprotect),
            Err(PlatformError::InvalidData)
        ));
        // Tampered data that "succeeds with corrupted output".
        let blob = STANDARD.encode(b"anything");
        assert!(matches!(
            unprotect_with(&blob, |_| Ok(vec![0xFF, 0xFE, 0x00])),
            Err(PlatformError::InvalidData)
        ));
        assert!(matches!(
            unprotect_with(&blob, |_| Ok(b"a\0b".to_vec())),
            Err(PlatformError::InvalidData)
        ));
        assert!(matches!(
            unprotect_with(&blob, |_| Ok(Vec::new())),
            Err(PlatformError::InvalidData)
        ));
        assert!(matches!(
            unprotect_with(&blob, |_| Ok(vec![b'a'; MAX_SECRET_BYTES + 1])),
            Err(PlatformError::InvalidData)
        ));
    }

    #[test]
    fn wipe_zeroes() {
        let mut v = b"password".to_vec();
        wipe(&mut v);
        assert!(v.iter().all(|&b| b == 0));
        wipe(&mut []);
    }
}
