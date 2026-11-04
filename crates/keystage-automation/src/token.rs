//! The control API's credential (docs/ARCHITECTURE.md section 14): a
//! random 256-bit token the app keeps in `control.json` in its local data
//! folder (`%LOCALAPPDATA%\com.ideabosco.keystage`, readable by the signed-in
//! user), sent as `Authorization: Bearer <token>`, compared in constant time.

use serde::{Deserialize, Serialize};

/// The file the app writes at every start and the connector reads.
pub const CONTROL_FILE: &str = "control.json";

/// What `control.json` holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlFile {
    pub port: u16,
    pub token: String,
    /// The app version that wrote it.
    #[serde(default)]
    pub version: String,
}

/// Lowercase hex of 32 random bytes.
pub fn token_from_bytes(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A token this app could have made: 64 lowercase hex digits.
pub fn is_token(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Equal bytes, in time that depends only on the lengths.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() ^ b.len()) as u8 | u8::from(a.len() != b.len());
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
}

/// True when an `Authorization` header carries `token`.
pub fn authorized(header: Option<&str>, token: &str) -> bool {
    let Some(h) = header else { return false };
    let Some(given) = h.strip_prefix("Bearer ") else {
        return false;
    };
    is_token(token) && constant_time_eq(given.trim().as_bytes(), token.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_and_headers() {
        let t = token_from_bytes(&[0xab; 32]);
        assert_eq!(t.len(), 64);
        assert!(is_token(&t));
        assert!(!is_token(&t.to_uppercase()));
        assert!(!is_token(&t[..63]));
        assert!(authorized(Some(&format!("Bearer {t}")), &t));
        assert!(!authorized(Some(&format!("bearer {t}")), &t));
        assert!(!authorized(Some(&t), &t), "the scheme is required");
        assert!(!authorized(None, &t));
        let wrong = token_from_bytes(&[0xac; 32]);
        assert!(!authorized(Some(&format!("Bearer {wrong}")), &t));
        assert!(!authorized(Some(&format!("Bearer {}", &t[..63])), &t));
        assert!(
            !authorized(Some("Bearer "), ""),
            "an empty token never matches"
        );
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"\0"));
        let f = ControlFile {
            port: 47820,
            token: t.clone(),
            version: "0.3.0".into(),
        };
        let back: ControlFile =
            serde_json::from_str(&serde_json::to_string(&f).expect("j")).expect("r");
        assert_eq!(back, f);
    }
}
