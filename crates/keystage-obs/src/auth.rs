//! obs-websocket 5 authentication and the password wrapper.
//!
//! The algorithm, verbatim from the protocol ("Creating an authentication
//! string", <https://raw.githubusercontent.com/obsproject/obs-websocket/master/docs/generated/protocol.md>,
//! read 2026-10-01):
//!
//! > - Concatenate the websocket password with the `salt` provided by the server (`password + salt`)
//! > - Generate an SHA256 binary hash of the result and base64 encode it, known as a base64 secret.
//! > - Concatenate the base64 secret with the `challenge` sent by the server (`base64_secret + challenge`)
//! > - Generate a binary SHA256 hash of that result and base64 encode it. You now have your `authentication` string.
//!
//! The challenge changes with every connection, so the string is computed
//! again each time.

use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Overwrites a buffer before it is freed (best effort: the optimizer is
/// asked not to drop the writes through `black_box`).
fn wipe(mut bytes: Vec<u8>) {
    bytes.fill(0);
    std::hint::black_box(&bytes);
}

/// The obs-websocket password. Its `Debug` output never shows it, and its
/// memory is overwritten when it is dropped (best effort: copies made by the
/// allocator or the OS are out of reach).
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(s: impl Into<String>) -> Password {
        Password(s.into())
    }

    /// The password text. Use it only to compute the authentication string.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<hidden>)")
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        wipe(std::mem::take(&mut self.0).into_bytes());
    }
}

/// `base64(sha256(base64(sha256(password + salt)) + challenge))`.
pub fn auth_string(password: &str, salt: &str, challenge: &str) -> String {
    let mut first = Vec::with_capacity(password.len() + salt.len());
    first.extend_from_slice(password.as_bytes());
    first.extend_from_slice(salt.as_bytes());
    let secret = STANDARD.encode(Sha256::digest(&first));
    wipe(first);
    let mut second = Vec::with_capacity(secret.len() + challenge.len());
    second.extend_from_slice(secret.as_bytes());
    second.extend_from_slice(challenge.as_bytes());
    wipe(secret.into_bytes());
    let out = STANDARD.encode(Sha256::digest(&second));
    wipe(second);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test vector from the Phase 1 research (`.work/research/obs.md`
    /// B.4): the protocol's Hello example salt and challenge with password
    /// `supersecretpassword`, computed with Python and cross-checked with
    /// openssl. The protocol's own Identify example does not match its Hello
    /// example, so it is not used.
    #[test]
    fn b4_test_vector() {
        let salt = "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=";
        let challenge = "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=";
        let secret = STANDARD.encode(Sha256::digest(format!("supersecretpassword{salt}")));
        assert_eq!(secret, "H1IfVz1pSREUQzbFTVnX/Tyb+gMhMik5x7yUBCY0PTs=");
        assert_eq!(
            auth_string("supersecretpassword", salt, challenge),
            "1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4="
        );
        assert_ne!(
            auth_string("supersecretpassword", salt, challenge),
            "Dj6cLS+jrNA0HpCArRg0Z/Fc+YHdt2FQfAvgD1mip6Y=",
            "the protocol's inconsistent Identify example"
        );
    }

    #[test]
    fn password_is_never_printed() {
        let p = Password::new("hunter22");
        assert_eq!(format!("{p:?}"), "Password(<hidden>)");
        assert_eq!(p.expose(), "hunter22");
        assert!(!p.is_empty());
        assert!(Password::new("").is_empty());
    }

    #[test]
    fn unicode_passwords_hash_as_utf8() {
        // Server side hashes the UTF-8 bytes (obs-websocket Crypto.cpp).
        let a = auth_string("pässwörd", "s", "c");
        let b = auth_string("p\u{e4}ssw\u{f6}rd", "s", "c");
        assert_eq!(a, b);
        assert_eq!(a.len(), 44);
    }
}
