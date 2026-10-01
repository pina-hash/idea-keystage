//! Built-in profiles, embedded at compile time (ARCHITECTURE.md section 9).
//!
//! `data/profiles/solidworks.json` is sourced data only: every entry cites
//! Dassault's SOLIDWORKS Design Help with a URL, version, date read and a
//! verbatim quote. Code never adds SolidWorks captions of its own.

use crate::profile::{Profile, ProfileError};

/// The SolidWorks profile file, byte for byte.
pub const SOLIDWORKS_JSON: &str = include_str!("../../../data/profiles/solidworks.json");

/// Parses the built-in SolidWorks profile. The app shell calls this once at
/// startup and shares the result in an `Arc`.
pub fn solidworks() -> Result<Profile, ProfileError> {
    Profile::from_json(SOLIDWORKS_JSON)
}

/// The general fallback profile.
pub fn general() -> Profile {
    Profile::general()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{CaptionIndex, SOLIDWORKS_ID};
    use crate::rules::TypingMode;

    #[test]
    fn embedded_solidworks_profile_parses() {
        let p = solidworks().expect("the embedded profile parses");
        assert_eq!(p.id, SOLIDWORKS_ID);
        assert_eq!(p.name, "SolidWorks");
        assert!(p.matches_process("SLDWORKS.exe"));
        assert!(p.matches_process("sldworks.EXE"));
        assert_eq!(p.display.mode, Some(TypingMode::All));
        assert!(!p.shortcuts.is_empty());
        assert!(!p.mouse.is_empty());
        let idx = CaptionIndex::build(&p, &[]);
        assert!(!idx.is_empty());
    }

    #[test]
    fn general_profile_is_empty() {
        let g = general();
        assert!(g.shortcuts.is_empty() && g.mouse.is_empty());
        assert!(g.matcher.process.is_empty());
    }
}
