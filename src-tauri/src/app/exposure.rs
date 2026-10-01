//! How much of what is typed a processor configuration can show
//! (ARCHITECTURE.md section 8, rule 3), and the masked profiles used while
//! UI Automation password detection is not running.
//!
//! The pipeline uses the [`Exposure`] order to tell a change that shows more
//! (leaving a password field or an excluded app, unlocking, switching to a
//! profile that shows typed text) from one that shows the same or less. A
//! change that shows more waits until the input that may have been typed
//! before it has been handled (`pipeline::Core::pass`).
//!
//! While UI Automation does not run, only classic Edit password boxes are
//! detected, so a password typed into a browser, WPF, UWP or Electron field
//! would not be suppressed. The app then fails closed by masking typing in
//! every profile ([`masked_rules`], [`masked_profile`]): typed text is never
//! shown, and neither are profile captions for bare typing keys (the
//! SolidWorks list captions `S`, `D`, `Space` and others, which would
//! otherwise show those letters one by one).

use keystage_core::profile::{keys_to_steps, Profile};
use keystage_core::{DisplayRules, TypingMode};

/// What a processor configuration can show of what is typed, from nothing
/// to typed text. A change to a higher level is a relaxation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Exposure {
    /// Suppressed: nothing at all is shown.
    Nothing,
    /// Typing is masked: only combos and special keys show.
    Shortcuts,
    /// Typing is masked, but the profile captions some bare typing keys
    /// (`S`, `Space`), and those show when pressed.
    CaptionedKeys,
    /// Typed text is shown as text runs.
    Text,
}

/// The exposure of a processor with these settings: `suppressed`, the
/// active profile (its `display` block applies on top of `base`), and the
/// general rules `base`.
pub fn exposure(suppressed: bool, profile: Option<&Profile>, base: &DisplayRules) -> Exposure {
    if suppressed {
        return Exposure::Nothing;
    }
    let rules = profile.map(|p| p.apply_display(base));
    let rules = rules.as_ref().unwrap_or(base);
    if rules.typing == TypingMode::All && rules.typed_values {
        Exposure::Text
    } else if profile.is_some_and(captions_typing_keys) {
        Exposure::CaptionedKeys
    } else {
        Exposure::Shortcuts
    }
}

/// True for canonical key names that never type a character: the keys
/// keystage-core classifies as `KeyClass::Special` (shown even when typing
/// is masked). Anything else, including names this list does not know,
/// counts as typing, so masking fails closed.
pub fn is_non_typing_key(name: &str) -> bool {
    const NAMED: &[&str] = &[
        "Esc",
        "Enter",
        "NumEnter",
        "Tab",
        "Backspace",
        "Delete",
        "Insert",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Up",
        "Down",
        "Left",
        "Right",
        "NumHome",
        "NumEnd",
        "NumPageUp",
        "NumPageDown",
        "NumUp",
        "NumDown",
        "NumLeft",
        "NumRight",
        "NumInsert",
        "NumDelete",
        "NumClear",
        "CapsLock",
        "PrintScreen",
        "ScrollLock",
        "Pause",
        "Menu",
        "NumLock",
        "VolumeUp",
        "VolumeDown",
        "VolumeMute",
        "MediaPlayPause",
        "MediaNext",
        "MediaPrev",
        "MediaStop",
        "BrowserBack",
        "BrowserForward",
        "BrowserRefresh",
        "BrowserHome",
        "BrowserSearch",
        "LaunchMail",
        "LaunchApp1",
        "LaunchApp2",
    ];
    if NAMED.contains(&name) {
        return true;
    }
    // F1 to F24.
    name.strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n) && !name.starts_with("F0"))
}

/// True for a shortcut entry with a step that a typing key makes without
/// Ctrl, Alt or Win (`S`, `Shift + Z`, `Space`, or a later step of a
/// sequence such as `Tab, S`). Entries the caption index cannot read are
/// never matched, so they do not count.
fn has_typing_step(keys: &[String]) -> bool {
    keys_to_steps(keys).is_some_and(|steps| {
        steps.iter().any(|step| {
            !step.mods.has_command() && step.key.as_deref().is_some_and(|k| !is_non_typing_key(k))
        })
    })
}

/// True when the profile captions a bare typing key, which then shows even
/// while typing is masked.
pub fn captions_typing_keys(profile: &Profile) -> bool {
    profile.shortcuts.iter().any(|s| has_typing_step(&s.keys))
}

/// The general rules with typing masked.
pub fn masked_rules(mut rules: DisplayRules) -> DisplayRules {
    rules.typing = TypingMode::ShortcutsOnly;
    rules.typed_values = false;
    rules
}

/// `profile` with typing masked and without the entries a bare typing key
/// would trigger. Combos, special keys and their captions are kept.
pub fn masked_profile(profile: &Profile) -> Profile {
    let mut p = profile.clone();
    p.display.mode = Some(TypingMode::ShortcutsOnly);
    p.display.typed_values = Some(false);
    p.shortcuts.retain(|s| !has_typing_step(&s.keys));
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystage_core::builtin;
    use keystage_core::profile::Shortcut;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_order_runs_from_nothing_to_text() {
        assert!(Exposure::Nothing < Exposure::Shortcuts);
        assert!(Exposure::Shortcuts < Exposure::CaptionedKeys);
        assert!(Exposure::CaptionedKeys < Exposure::Text);
    }

    #[test]
    fn exposure_of_the_built_in_profiles() {
        let base = DisplayRules::default();
        let sw = builtin::solidworks().expect("profile");
        let general = Profile::general();
        assert_eq!(exposure(true, Some(&sw), &base), Exposure::Nothing);
        assert_eq!(exposure(false, Some(&sw), &base), Exposure::Text);
        assert_eq!(exposure(false, Some(&general), &base), Exposure::Shortcuts);
        assert_eq!(exposure(false, None, &base), Exposure::Shortcuts);
        let mut no_values = sw.clone();
        no_values.display.typed_values = Some(false);
        assert_eq!(
            exposure(false, Some(&no_values), &base),
            Exposure::CaptionedKeys,
            "S, D, Space and others are captioned bare keys"
        );
        let mut typing = base.clone();
        typing.typing = TypingMode::All;
        assert_eq!(exposure(false, None, &typing), Exposure::Text);
    }

    #[test]
    fn typing_keys_are_told_from_special_keys() {
        for k in [
            "Esc", "Enter", "Tab", "F1", "F12", "F24", "Up", "NumHome", "Menu",
        ] {
            assert!(is_non_typing_key(k), "{k}");
        }
        for k in [
            "A", "S", "0", "Space", "-", "`", "Num1", "NumAdd", "F", "F0", "F25", "VK_E8",
        ] {
            assert!(!is_non_typing_key(k), "{k}");
        }
        assert!(has_typing_step(&keys(&["S"])));
        assert!(has_typing_step(&keys(&["Shift", "Z"])));
        assert!(has_typing_step(&keys(&["Tab", "S"])));
        assert!(!has_typing_step(&keys(&["Ctrl", "S"])));
        assert!(!has_typing_step(&keys(&["Alt", "Shift", "S"])));
        assert!(!has_typing_step(&keys(&["F5"])));
        assert!(!has_typing_step(&keys(&["Shift", "Tab"])));
        assert!(!has_typing_step(&keys(&["Shift"])));
        assert!(!has_typing_step(&keys(&["Nonsense"])));
    }

    #[test]
    fn the_masked_solidworks_profile_keeps_combos_and_special_keys() {
        let sw = builtin::solidworks().expect("profile");
        let masked = masked_profile(&sw);
        let base = DisplayRules::default();
        assert_eq!(exposure(false, Some(&masked), &base), Exposure::Shortcuts);
        assert_eq!(
            exposure(false, Some(&masked), &masked_rules(base.clone())),
            Exposure::Shortcuts
        );
        assert!(!captions_typing_keys(&masked));
        let has = |p: &Profile, k: &[&str]| p.shortcuts.iter().any(|s| s.keys == keys(k));
        assert!(has(&sw, &["S"]) && !has(&masked, &["S"]));
        assert!(has(&sw, &["Ctrl", "B"]) && has(&masked, &["Ctrl", "B"]));
        assert!(has(&masked, &["Esc"]) || !has(&sw, &["Esc"]));
        assert_eq!(masked.id, sw.id);
        assert!(masked.shortcuts.len() < sw.shortcuts.len());
    }

    #[test]
    fn a_user_profile_with_bare_letter_captions_shows_more_than_shortcuts() {
        let mut p = Profile::general();
        p.id = "user".into();
        p.shortcuts.push(Shortcut {
            keys: keys(&["G"]),
            command: "Grab".into(),
            ..Shortcut::default()
        });
        let base = DisplayRules::default();
        assert_eq!(exposure(false, Some(&p), &base), Exposure::CaptionedKeys);
        assert_eq!(
            exposure(false, Some(&masked_profile(&p)), &base),
            Exposure::Shortcuts
        );
    }

    #[test]
    fn masked_rules_mask_typing() {
        let m = masked_rules(DisplayRules {
            typing: TypingMode::All,
            typed_values: true,
            ..DisplayRules::default()
        });
        assert_eq!(m.typing, TypingMode::ShortcutsOnly);
        assert!(!m.typed_values);
    }
}
