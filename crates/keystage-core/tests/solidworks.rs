//! The SolidWorks profile against the real data file. Every expectation is
//! read from `data/profiles/solidworks.json` in the test, never typed in.

mod common;

use common::*;
use keystage_core::modifiers::ModSet;
use keystage_core::profile::{keys_to_steps, CaptionIndex, Direction, MouseAction, MouseQuery};
use keystage_core::{keys, CalloutKind, DisplayRules, MouseButton, Profile, TypingMode};

fn data() -> serde_json::Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/profiles/solidworks.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn entry_keys(e: &serde_json::Value) -> Vec<String> {
    e["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_file_round_trips_exactly() {
    let p = sw_profile();
    let back = serde_json::to_value(&p).unwrap();
    assert_eq!(back, data(), "the serde model matches the file on disk");
}

#[test]
fn every_key_name_is_canonical() {
    let d = data();
    let mut bad = Vec::new();
    for e in d["shortcuts"].as_array().unwrap() {
        for k in entry_keys(e) {
            if !keys::is_canonical(&k) {
                bad.push(format!("{} {}", e["id"], k));
            }
        }
    }
    assert!(bad.is_empty(), "non-canonical key names: {bad:?}");
}

#[test]
fn every_entry_is_indexed_except_unknown_mouse_actions() {
    let d = data();
    let unknown_actions = d["mouse"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| MouseAction::parse(m["action"].as_str().unwrap()).is_none())
        .count();
    let idx = CaptionIndex::build(&sw_profile(), &[]);
    assert_eq!(
        idx.skipped, unknown_actions,
        "only entries with an action outside the five are skipped"
    );
}

#[test]
fn every_keyboard_caption_matches_the_reference_lookup() {
    let d = data();
    let idx = CaptionIndex::build(&sw_profile(), &[]);
    let mut checked = 0;
    for e in d["shortcuts"].as_array().unwrap() {
        let ks = entry_keys(e);
        let steps = keys_to_steps(&ks).unwrap();
        let got = idx
            .lookup(&steps)
            .map(|c| (c.command.clone(), c.context.clone()));
        let refs: Vec<&str> = ks.iter().map(String::as_str).collect();
        let want = reference_caption(&refs);
        assert_eq!(got, want, "keys {ks:?}");
        checked += 1;
    }
    assert!(checked > 100);
}

#[test]
fn every_mouse_caption_matches_the_reference_lookup() {
    let d = data();
    let idx = CaptionIndex::build(&sw_profile(), &[]);
    let buttons = [
        ("left", MouseButton::Left),
        ("right", MouseButton::Right),
        ("middle", MouseButton::Middle),
    ];
    let mut checked = 0;
    for m in d["mouse"].as_array().unwrap() {
        let Some(action) = MouseAction::parse(m["action"].as_str().unwrap()) else {
            continue;
        };
        let mods: Vec<&str> = m["modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let dirs: Vec<Option<&str>> = match action {
            MouseAction::Wheel => vec![Some("up"), Some("down")],
            MouseAction::Gesture => vec![m["direction"].as_str()],
            _ => vec![None],
        };
        for (bname, b) in buttons {
            for dir in &dirs {
                let q = MouseQuery {
                    button: b,
                    action,
                    mods: ModSet::from_names(mods.iter().copied()).unwrap(),
                    direction: dir.and_then(Direction::parse),
                };
                let got = idx
                    .lookup_mouse(&q)
                    .map(|c| (c.command.clone(), c.context.clone()));
                let want =
                    reference_mouse_caption(bname, m["action"].as_str().unwrap(), &mods, *dir);
                assert_eq!(got, want, "{} {bname} {dir:?}", m["id"]);
                checked += 1;
            }
        }
    }
    assert!(checked > 100);
}

/// A bare letter that has a profile entry, and one that has none.
fn letters() -> (String, String) {
    let d = data();
    let bare: Vec<String> = d["shortcuts"]
        .as_array()
        .unwrap()
        .iter()
        .map(entry_keys)
        .filter(|k| k.len() == 1 && k[0].len() == 1 && k[0].as_bytes()[0].is_ascii_uppercase())
        .map(|k| k[0].clone())
        .collect();
    let with = bare
        .first()
        .expect("the data has a bare letter shortcut")
        .clone();
    let without = ('A'..='Z')
        .map(|c| c.to_string())
        .find(|c| !bare.contains(c))
        .expect("a letter with no entry");
    (with, without)
}

#[test]
fn bare_shortcut_keys_show_with_their_caption() {
    let (with, _) = letters();
    let mut h = H::solidworks();
    let out = h.tap(&with);
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Key);
    assert_eq!(c.profile.as_deref(), Some("solidworks"));
    let (command, context) = reference_caption(&[with.as_str()]).unwrap();
    assert_eq!(c.caption.as_deref(), Some(command.as_str()));
    assert_eq!(c.context, context);
    assert!(key_events(&out).contains(&(with.clone(), true)));
    // Never merged into a text run, even mid-run.
    let mut h = H::solidworks();
    h.type_keys(&["1", "2"]);
    let out = h.tap(&with);
    assert_eq!(last(&out).kind, CalloutKind::Key);
    assert_eq!(h.callouts().len(), 2);
}

#[test]
fn typed_values_form_text_runs() {
    let (_, without) = letters();
    let mut h = H::solidworks();
    h.type_keys(&["2", "5", ".", "4", "Enter"]);
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.kind, CalloutKind::Text);
    assert_eq!(c.text_form(), "25.4 Enter");
    // '-' is a typed value even though the data has a bare entry for it.
    let mut h = H::solidworks();
    h.type_keys(&["-", "3"]);
    assert_eq!(h.callouts().pop().unwrap().text.as_deref(), Some("-3"));
    // A letter that is not a shortcut types.
    let mut h = H::solidworks();
    let out = h.tap(&without);
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Text);
    assert_eq!(
        c.text.as_deref(),
        Some(without.to_ascii_lowercase().as_str())
    );
}

#[test]
fn typed_values_off_masks_but_keeps_shortcuts() {
    let (with, without) = letters();
    let mut p = sw_profile();
    p.display.typed_values = Some(false);
    let mut h = H::with_profile(DisplayRules::default(), p);
    assert_eq!(h.p.rules().typing, TypingMode::All);
    let out = h.type_keys(&["2", "5", ".", "4"]);
    assert!(callouts_in(&out)
        .iter()
        .all(|c| c.kind == CalloutKind::Masked));
    assert!(key_events(&out).is_empty());
    h.wait(2000);
    let out = h.tap(&without);
    assert_eq!(last(&out).kind, CalloutKind::Masked);
    let out = h.tap(&with);
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Key);
    assert_eq!(
        c.caption.as_deref(),
        Some(reference_caption(&[with.as_str()]).unwrap().0.as_str())
    );
}

#[test]
fn shift_letter_shortcuts_are_combos_in_solidworks() {
    let d = data();
    let combo = d["shortcuts"]
        .as_array()
        .unwrap()
        .iter()
        .map(entry_keys)
        .find(|k| {
            k.len() == 2
                && k[0] == "Shift"
                && k[1].len() == 1
                && k[1].as_bytes()[0].is_ascii_uppercase()
        })
        .expect("the data has a Shift + letter shortcut");
    let mut h = H::solidworks();
    let out = h.combo(&["Shift"], &combo[1]);
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Combo);
    assert_eq!(
        c.caption.as_deref(),
        Some(
            reference_caption(&["Shift", combo[1].as_str()])
                .unwrap()
                .0
                .as_str()
        )
    );
}

#[test]
fn ctrl_combos_get_captions() {
    let d = data();
    let mut tried = 0;
    for ks in d["shortcuts"].as_array().unwrap().iter().map(entry_keys) {
        if ks.len() != 2
            || ks[0] != "Ctrl"
            || ks[1].len() != 1
            || !ks[1].as_bytes()[0].is_ascii_uppercase()
        {
            continue;
        }
        let mut h = H::solidworks();
        let out = h.combo(&["Ctrl"], &ks[1]);
        let c = last(&out);
        let (command, context) = reference_caption(&["Ctrl", ks[1].as_str()]).unwrap();
        assert_eq!(c.caption.as_deref(), Some(command.as_str()), "{ks:?}");
        assert_eq!(c.context, context);
        tried += 1;
    }
    assert!(tried >= 3);
}

#[test]
fn alt_tap_chord_reaches_sequence_entries() {
    let d = data();
    let seq = d["shortcuts"]
        .as_array()
        .unwrap()
        .iter()
        .map(entry_keys)
        .find(|k| {
            k.len() == 3
                && k[0] == "Alt"
                && k[1..]
                    .iter()
                    .all(|x| x.len() == 1 && x.as_bytes()[0].is_ascii_uppercase())
        })
        .expect("the data has an Alt, letter, letter sequence");
    let refs: Vec<&str> = seq.iter().map(String::as_str).collect();
    let (command, _) = reference_caption(&refs).unwrap();
    let mut h = H::solidworks();
    h.tap("Alt");
    h.type_keys(&[&seq[1], &seq[2]]);
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.kind, CalloutKind::Chord);
    assert_eq!(c.caption.as_deref(), Some(command.as_str()));
    // The same sequence as Alt+letter then letter.
    let mut h = H::solidworks();
    h.combo(&["Alt"], &seq[1]);
    h.tap(&seq[2]);
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.kind, CalloutKind::Chord);
    assert_eq!(c.caption.as_deref(), Some(command.as_str()));
}

fn mouse_caption(button: &str, action: &str, mods: &[&str], dir: Option<&str>) -> Option<String> {
    reference_mouse_caption(button, action, mods, dir).map(|c| c.0)
}

#[test]
fn mouse_callouts_use_the_mouse_table() {
    let mut h = H::solidworks();
    assert!(
        h.p.rules().mouse_callouts,
        "the profile turns mouse callouts on"
    );
    // Middle drag.
    h.button(MouseButton::Middle, true, 500, 500);
    let out = h.mv(560, 500);
    h.button(MouseButton::Middle, false, 560, 500);
    let c = last(&out);
    assert_eq!(c.text_form(), "Middle drag");
    assert_eq!(c.caption, mouse_caption("middle", "drag", &[], None));
    assert!(c.caption.is_some());
    // Each modifier the table uses with a middle drag.
    for mods in [
        vec!["Ctrl"],
        vec!["Shift"],
        vec!["Alt"],
        vec!["Ctrl", "Alt"],
    ] {
        h.wait(2000);
        for m in &mods {
            h.down(m);
        }
        h.button(MouseButton::Middle, true, 500, 500);
        let out = h.mv(560, 500);
        h.button(MouseButton::Middle, false, 560, 500);
        for m in mods.iter().rev() {
            h.up(m);
        }
        let c = last(&out);
        assert_eq!(
            c.caption,
            mouse_caption("middle", "drag", &mods, None),
            "{mods:?}"
        );
    }
    // Wheel up and down share the wheel entries.
    h.wait(2000);
    let up = last(&h.wheel(0, 120));
    let down = last(&h.wheel(0, -120));
    assert_eq!(
        up.caption,
        mouse_caption("middle", "wheel", &[], Some("up"))
    );
    assert_eq!(down.caption, up.caption);
    assert!(up.caption.is_some());
    // An unspecified-button entry captions a left drag with that modifier.
    h.wait(2000);
    h.down("Ctrl");
    h.button(MouseButton::Left, true, 100, 100);
    let out = h.mv(200, 100);
    h.button(MouseButton::Left, false, 200, 100);
    h.up("Ctrl");
    let c = last(&out);
    assert_eq!(c.text_form(), "Ctrl + Left drag");
    assert_eq!(c.caption, mouse_caption("left", "drag", &["Ctrl"], None));
    assert!(c.caption.is_some());
}

#[test]
fn gestures_are_captioned_only_from_sourced_directions() {
    let d = data();
    let sourced: Vec<String> = d["mouse"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| {
            m["action"] == "gesture"
                && m["direction"].is_string()
                && m["modifiers"].as_array().unwrap().is_empty()
        })
        .map(|m| m["direction"].as_str().unwrap().to_string())
        .collect();
    assert!(!sourced.is_empty());
    let vectors = [
        ("up", (0, -80)),
        ("up_right", (80, -80)),
        ("right", (80, 0)),
        ("down_right", (80, 80)),
        ("down", (0, 80)),
        ("down_left", (-80, 80)),
        ("left", (-80, 0)),
        ("up_left", (-80, -80)),
    ];
    let mut h = H::solidworks();
    for (dir, (dx, dy)) in vectors {
        h.wait(2000);
        h.button(MouseButton::Right, true, 500, 500);
        h.mv(500 + dx / 2, 500 + dy / 2);
        let out = h.button(MouseButton::Right, false, 500 + dx, 500 + dy);
        let c = last(&out);
        let want = mouse_caption("right", "gesture", &[], Some(dir));
        assert_eq!(c.caption, want, "{dir}");
        assert_eq!(want.is_some(), sourced.contains(&dir.to_string()), "{dir}");
    }
}

#[test]
fn process_matching_uses_the_file() {
    let p: Profile = sw_profile();
    let d = data();
    for exe in d["match"]["process"].as_array().unwrap() {
        let exe = exe.as_str().unwrap();
        assert!(p.matches_process(exe));
        assert!(p.matches_process(&exe.to_ascii_lowercase()));
        assert!(p.matches_process(&format!(
            "C:\\Program Files\\SOLIDWORKS Corp\\SOLIDWORKS\\{exe}"
        )));
    }
    assert!(!p.matches_process("notepad.exe"));
}
