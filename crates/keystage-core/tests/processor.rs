//! Section 4 rules, one test (or a few) per rule.

mod common;

use std::sync::Arc;

use common::*;
use keystage_core::{
    CalloutKind, ChipKind, DisplayEvent, DisplayRules, InputEvent, KeyInput, LegendProvider, Mod,
    MonitorRect, MouseButton, PhysKey, Profile, Side, Suppress, TextMods, TypingMode,
};

fn all_mode() -> DisplayRules {
    DisplayRules {
        typing: TypingMode::All,
        typed_values: true,
        ..DisplayRules::default()
    }
}

#[test]
fn combo_makes_one_callout_in_modifier_order() {
    let mut h = H::general();
    let out = h.combo(&["Ctrl", "Shift"], "S");
    let cs = callouts_in(&out);
    assert_eq!(cs.len(), 1);
    let c = &cs[0];
    assert_eq!(c.kind, CalloutKind::Combo);
    assert_eq!(c.text_form(), "Ctrl + Shift + S");
    assert_eq!(c.steps.len(), 1);
    let kinds: Vec<_> = c.steps[0].iter().map(|ch| ch.kind).collect();
    assert_eq!(
        kinds,
        vec![ChipKind::Modifier, ChipKind::Modifier, ChipKind::Key]
    );
    assert_eq!(c.count, 1);
    assert_eq!(c.caption, None, "the general profile has no captions");
    // Pressing Shift first gives the same order.
    h.wait(2000);
    let out = h.combo(&["Shift", "Ctrl"], "S");
    assert_eq!(last(&out).text_form(), "Ctrl + Shift + S");
    // Alt, AltGr and Win come after Ctrl and Shift.
    h.wait(2000);
    let out = h.combo(&["Win", "Alt", "Ctrl"], "D");
    assert_eq!(last(&out).text_form(), "Ctrl + Alt + Win + D");
}

#[test]
fn key_events_light_and_release_every_key() {
    let mut h = H::general();
    let out = h.combo(&["Ctrl"], "S");
    assert_eq!(
        key_events(&out),
        vec![
            ("Ctrl".into(), true),
            ("S".into(), true),
            ("S".into(), false),
            ("Ctrl".into(), false)
        ]
    );
    let w = wire(&out);
    assert!(
        w[0].contains(r#""phys":"sc:1D""#) && w[0].contains(r#""modifier":"ctrl""#),
        "{}",
        w[0]
    );
}

#[test]
fn modifier_side_labels() {
    let rules = DisplayRules {
        show_modifier_side: true,
        ..DisplayRules::default()
    };
    let mut h = H::new(rules);
    let out = h.combo(&["Ctrl"], "S");
    assert_eq!(last(&out).text_form(), "Left Ctrl + S");
    h.wait(2000);
    let out = h.combo(&["RCtrl"], "S");
    let c = last(&out);
    assert_eq!(c.text_form(), "Right Ctrl + S");
    assert_eq!(c.steps[0][0].key, "Ctrl", "the key stays canonical");
}

#[test]
fn repeats_collapse_into_one_callout() {
    let mut h = H::general();
    let first = last(&h.combo(&["Ctrl"], "Z"));
    for _ in 0..3 {
        h.combo(&["Ctrl"], "Z");
    }
    let finals = h.callouts();
    assert_eq!(finals.len(), 1);
    assert_eq!(finals[0].id, first.id);
    assert_eq!(finals[0].count, 4);
    assert_eq!(finals[0].text_form(), "Ctrl + Z x4");
    // Holding Ctrl and tapping Z collapses too.
    h.down("Ctrl");
    h.tap("Z");
    h.tap("Z");
    h.up("Ctrl");
    assert_eq!(h.callouts()[0].count, 6);
    // After the repeat window a new callout starts.
    h.wait(1500);
    let out = h.combo(&["Ctrl"], "Z");
    let c = last(&out);
    assert_ne!(c.id, first.id);
    assert_eq!(c.count, 1);
    // A different combo in between breaks the run.
    h.combo(&["Ctrl"], "Y");
    let out = h.combo(&["Ctrl"], "Z");
    assert_eq!(last(&out).count, 1);
}

#[test]
fn auto_repeat_counts_on_the_same_callout_without_key_events() {
    let mut h = H::general();
    h.down("Ctrl");
    let out = h.down("Z");
    let id = last(&out).id;
    let mut repeat_events = Vec::new();
    h.step = 500;
    repeat_events.extend(h.down("Z"));
    h.step = 33;
    for _ in 0..5 {
        repeat_events.extend(h.down("Z"));
    }
    let c = last(&repeat_events);
    assert_eq!(c.id, id);
    assert_eq!(c.count, 7);
    assert!(
        key_events(&repeat_events).is_empty(),
        "auto-repeat sends no key events"
    );
    h.up("Z");
    h.up("Ctrl");
    // A held single key: Down x5.
    let mut h = H::general();
    let mut out = h.down("Down");
    for _ in 0..4 {
        out.extend(h.down("Down"));
    }
    out.extend(h.up("Down"));
    assert_eq!(h.callouts().len(), 1);
    assert_eq!(h.callouts()[0].text_form(), "Down x5");
    assert_eq!(
        key_events(&out),
        vec![("Down".into(), true), ("Down".into(), false)]
    );
}

#[test]
fn single_keys_collapse_and_break() {
    let mut h = H::general();
    h.type_keys(&["F5", "F5"]);
    assert_eq!(h.callouts().len(), 1);
    assert_eq!(h.callouts()[0].text_form(), "F5 x2");
    h.type_keys(&["F6", "F5"]);
    let all = h.callouts();
    assert_eq!(all.len(), 3);
    assert_eq!(all[1].text_form(), "F6");
    assert_eq!(all[2].text_form(), "F5");
}

#[test]
fn alt_tap_starts_a_chord() {
    let mut h = H::general();
    h.tap("Alt");
    h.type_keys(&["F", "C"]);
    let all = h.callouts();
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].kind, CalloutKind::Chord);
    assert_eq!(all[0].text_form(), "Alt, F, C");
    assert_eq!(all[0].steps.len(), 3);
}

#[test]
fn chord_window_and_length_are_bounded() {
    // Too slow: the letter after the window is ordinary typing (masked).
    let mut h = H::general();
    h.tap("Alt");
    h.wait(1000);
    let out = h.tap("F");
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Masked);
    assert!(key_events(&out).is_empty());
    // At most chord_max_steps (4) steps: Alt plus three keys.
    let mut h = H::general();
    h.tap("Alt");
    h.type_keys(&["H", "O", "I"]);
    let out = h.tap("D");
    let all = h.callouts();
    assert_eq!(all[0].text_form(), "Alt, H, O, I");
    assert_eq!(
        last(&out).kind,
        CalloutKind::Masked,
        "the fifth key is typing"
    );
    // A combo after the tap is its own callout.
    let mut h = H::general();
    h.tap("Alt");
    let out = h.combo(&["Ctrl"], "S");
    assert_eq!(last(&out).text_form(), "Ctrl + S");
    assert_eq!(h.callouts().len(), 2);
}

fn chord_profile() -> Profile {
    let p = r#"{
        "schema": "keystage.profile/1", "id": "fixture", "name": "Fixture",
        "match": {"process": ["fixture.exe"]},
        "chords": [["Ctrl", "K"]],
        "shortcuts": [
            {"id": "f-1", "keys": ["Ctrl", "K", "Ctrl", "C"], "command": "Fixture comment", "context": ""},
            {"id": "f-2", "keys": ["Alt", "F", "X"], "command": "Fixture sequence", "context": ""}
        ]
    }"#;
    Profile::from_json(p).unwrap()
}

#[test]
fn profile_chord_leaders() {
    let mut h = H::with_profile(DisplayRules::default(), chord_profile());
    h.combo(&["Ctrl"], "K");
    h.combo(&["Ctrl"], "C");
    let all = h.callouts();
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].kind, CalloutKind::Chord);
    assert_eq!(all[0].text_form(), "Ctrl + K, Ctrl + C");
    assert_eq!(all[0].caption.as_deref(), Some("Fixture comment"));
    assert_eq!(all[0].profile.as_deref(), Some("fixture"));
    // Without the leader the combo stands alone.
    h.wait(2000);
    h.combo(&["Ctrl"], "C");
    assert_eq!(h.callouts().len(), 2);
    // A multi-step entry makes its first combo a leader too.
    h.wait(2000);
    h.combo(&["Alt"], "F");
    h.tap("X");
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.text_form(), "Alt + F, X");
    assert_eq!(c.caption.as_deref(), Some("Fixture sequence"));
    // An Alt tap chord reaches the same entry.
    h.wait(2000);
    h.tap("Alt");
    h.type_keys(&["F", "X"]);
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.text_form(), "Alt, F, X");
    assert_eq!(c.caption.as_deref(), Some("Fixture sequence"));
}

#[test]
fn text_runs_with_backspace_and_enter() {
    let mut h = H::new(all_mode());
    h.type_keys(&["2", "5", ".", "5", "Backspace", "4", "Enter"]);
    let all = h.callouts();
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].kind, CalloutKind::Text);
    assert_eq!(all[0].text.as_deref(), Some("25.4"));
    assert_eq!(all[0].text_form(), "25.4 Enter");
    // After Enter the run is closed: the next character starts a new one.
    h.tap("7");
    assert_eq!(h.callouts().len(), 2);
    // Tab closes too.
    let mut h = H::new(all_mode());
    h.type_keys(&["1", "Tab"]);
    assert_eq!(h.callouts()[0].text_form(), "1 Tab");
    // Shift types capitals; a gap closes the run.
    let mut h = H::new(all_mode());
    h.combo(&["Shift"], "H");
    h.type_keys(&["I"]);
    assert_eq!(h.callouts()[0].text.as_deref(), Some("Hi"));
    h.wait(2000);
    h.tap("A");
    assert_eq!(h.callouts().len(), 2);
    assert_eq!(h.callouts()[1].text.as_deref(), Some("a"));
}

#[test]
fn text_run_backspace_auto_repeat_and_empty_run() {
    let mut h = H::new(all_mode());
    h.type_keys(&["1", "2", "3"]);
    h.down("Backspace");
    h.step = 500;
    h.down("Backspace");
    h.step = 30;
    h.down("Backspace");
    h.down("Backspace");
    h.up("Backspace");
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.text.as_deref(), Some(""));
    assert_eq!(h.callouts().len(), 1);
    // Backspace with no open run is a key callout.
    h.wait(2000);
    let out = h.tap("Backspace");
    assert_eq!(last(&out).text_form(), "Backspace");
}

#[test]
fn masked_typing_never_reveals_keys() {
    let mut h = H::general();
    let mut out = h.type_keys(&["H", "E", "L", "L", "O", "Space", "W"]);
    out.extend(h.combo(&["Shift"], "Q"));
    out.extend(h.type_keys(&["1", "2", ".", "-", "Num7", "NumDecimal", "Backspace"]));
    // Only modifiers ever produce key events.
    for (k, _) in key_events(&out) {
        assert_eq!(k, "Shift", "only the modifier is lit");
    }
    let cs = callouts_in(&out);
    assert!(!cs.is_empty());
    let id = cs[0].id;
    for c in &cs {
        assert_eq!(c.kind, CalloutKind::Masked);
        assert_eq!(c.id, id, "one masked callout per run");
        assert!(c.steps.iter().all(|s| s.is_empty()), "no key identity");
        assert_eq!(c.text, None);
        assert_eq!(c.count, 1);
        assert_eq!(c.text_form(), "Typing");
    }
    // Nothing on the wire names a typed key.
    for line in wire(&out) {
        for letter in [
            "\"H\"",
            "\"E\"",
            "\"L\"",
            "\"O\"",
            "\"W\"",
            "\"Q\"",
            "\"1\"",
            "\"Num7\"",
            "\"Space\"",
        ] {
            assert!(!line.contains(letter), "{line} reveals {letter}");
        }
    }
    // Enter closes the run and shows as a trailing chip.
    let out = h.tap("Enter");
    let c = last(&out);
    assert_eq!(c.id, id);
    assert_eq!(c.text_form(), "Typing Enter");
}

#[test]
fn masked_run_keepalive_is_rate_limited() {
    let mut h = H::general();
    h.step = 50;
    let mut out = Vec::new();
    for i in 0..60 {
        out.extend(h.tap(["A", "S", "D", "F"][i % 4]));
    }
    // 60 keys over about 6 seconds: one create plus at most one update per second.
    let cs = callouts_in(&out);
    assert!(
        cs.len() >= 2 && cs.len() <= 8,
        "{} masked updates",
        cs.len()
    );
    assert!(cs.iter().all(|c| c.id == cs[0].id));
    let stats = h.p.stats();
    assert_eq!(stats.masked, 60);
    assert_eq!(stats.shown, 0);
}

#[test]
fn special_keys_and_combos_show_while_typing_is_masked() {
    let mut h = H::general();
    for k in [
        "F5",
        "Esc",
        "Enter",
        "Tab",
        "Backspace",
        "Delete",
        "Up",
        "Home",
        "PageDown",
        "Insert",
        "VolumeUp",
        "MediaPlayPause",
    ] {
        h.wait(2000);
        let out = h.tap(k);
        let c = last(&out);
        assert_eq!(c.kind, CalloutKind::Key, "{k}");
        assert_eq!(c.steps[0][0].key, k);
        assert!(key_events(&out).contains(&(k.to_string(), true)));
    }
    h.wait(2000);
    let out = h.combo(&["Ctrl"], "C");
    assert_eq!(last(&out).text_form(), "Ctrl + C");
    let out = h.combo(&["Shift"], "Tab");
    assert_eq!(last(&out).text_form(), "Shift + Tab");
}

#[test]
fn media_keys_collapse() {
    let mut h = H::general();
    h.type_keys(&["VolumeUp", "VolumeUp", "VolumeUp"]);
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.text_form(), "Volume Up x3");
    assert_eq!(c.steps[0][0].key, "VolumeUp");
}

#[test]
fn win_key() {
    let mut h = H::general();
    let out = h.tap("Win");
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Key);
    assert_eq!(c.text_form(), "Win");
    assert_eq!(
        key_events(&out),
        vec![("Win".into(), true), ("Win".into(), false)]
    );
    let w = wire(&out);
    assert!(w[0].contains(r#""phys":"sc:E05B""#) && w[0].contains(r#""modifier":"win""#));
    h.wait(2000);
    assert_eq!(last(&h.combo(&["Win"], "E")).text_form(), "Win + E");
    h.wait(2000);
    assert_eq!(
        last(&h.combo(&["Win", "Shift"], "S")).text_form(),
        "Shift + Win + S"
    );
}

#[test]
fn solo_modifier_taps() {
    let mut h = H::general();
    assert_eq!(last(&h.tap("Ctrl")).text_form(), "Ctrl");
    h.wait(2000);
    h.down("Ctrl");
    h.down("Shift");
    h.up("Shift");
    let out = h.up("Ctrl");
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Combo);
    assert_eq!(c.text_form(), "Ctrl + Shift");
    // Shift used for typing is not a tap.
    let mut h = H::general();
    let out = h.combo(&["Shift"], "A");
    assert!(callouts_in(&out)
        .iter()
        .all(|c| c.kind == CalloutKind::Masked));
    // A click while Ctrl is held is not a tap either.
    let mut h = H::general();
    h.down("Ctrl");
    h.button(MouseButton::Left, true, 10, 10);
    h.button(MouseButton::Left, false, 10, 10);
    let out = h.up("Ctrl");
    assert!(callouts_in(&out).is_empty());
}

#[test]
fn altgr_by_scan_code() {
    let mut h = H::general();
    let mut out = Vec::new();
    h.t += 10;
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA2,
        phys: PhysKey::new(0x21D, false),
        down: true,
        injected: false,
        t_ms: h.t,
    })));
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA5,
        phys: PhysKey::new(0x38, true),
        down: true,
        injected: false,
        t_ms: h.t,
    })));
    out.extend(h.tap("Q"));
    h.t += 10;
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA2,
        phys: PhysKey::new(0x21D, false),
        down: false,
        injected: false,
        t_ms: h.t,
    })));
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA5,
        phys: PhysKey::new(0x38, true),
        down: false,
        injected: false,
        t_ms: h.t,
    })));
    assert_eq!(
        key_events(&out),
        vec![("AltGr".into(), true), ("AltGr".into(), false)],
        "no Ctrl, no Q"
    );
    let w = wire(&out);
    assert!(
        w[0].contains(r#""phys":"sc:E038""#) && w[0].contains(r#""modifier":"alt""#),
        "{}",
        w[0]
    );
    let cs = callouts_in(&out);
    assert!(
        cs.iter().all(|c| c.kind == CalloutKind::Masked),
        "AltGr+Q types a character: {cs:?}"
    );
    assert_eq!(h.p.stats().altgr_fake_ctrl, 2);
}

struct German;
impl LegendProvider for German {
    fn legend(&self, vk: u16, _phys: PhysKey) -> Option<String> {
        (vk == 0x51).then(|| "Q".to_string())
    }
    fn text(&self, vk: u16, _phys: PhysKey, mods: TextMods) -> Option<String> {
        match (vk, mods.altgr, mods.shift) {
            (0x51, true, _) => Some("@".into()),
            (0x51, false, false) => Some("q".into()),
            (0x51, false, true) => Some("Q".into()),
            _ => None,
        }
    }
}

#[test]
fn altgr_by_timestamp_fallback_types_text() {
    let mut h = H::new(all_mode());
    h.legends = Box::new(German);
    let t = h.t + 10;
    let mut out = Vec::new();
    // Same timestamp, ordinary left Ctrl scan code: the fallback applies.
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA2,
        phys: PhysKey::new(0x1D, false),
        down: true,
        injected: false,
        t_ms: t,
    })));
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA5,
        phys: PhysKey::new(0x38, true),
        down: true,
        injected: false,
        t_ms: t,
    })));
    h.t = t;
    out.extend(h.tap("Q"));
    let t2 = h.t + 10;
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA2,
        phys: PhysKey::new(0x1D, false),
        down: false,
        injected: false,
        t_ms: t2,
    })));
    out.extend(h.ev(InputEvent::Key(KeyInput {
        vk: 0xA5,
        phys: PhysKey::new(0x38, true),
        down: false,
        injected: false,
        t_ms: t2,
    })));
    assert!(
        !key_events(&out).iter().any(|(k, _)| k == "Ctrl"),
        "{:?}",
        key_events(&out)
    );
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.kind, CalloutKind::Text);
    assert_eq!(c.text.as_deref(), Some("@"));
}

#[test]
fn real_ctrl_then_right_alt_is_not_altgr() {
    let mut h = H::general();
    h.down("Ctrl");
    h.down("RAlt");
    let out = h.tap("Delete");
    assert_eq!(last(&out).text_form(), "Ctrl + Alt + Delete");
    h.up("RAlt");
    h.up("Ctrl");
}

#[test]
fn print_screen_key_up_only_is_a_press() {
    let mut h = H::general();
    let out = h.up("PrintScreen");
    let c = last(&out);
    assert_eq!(c.kind, CalloutKind::Key);
    assert_eq!(c.steps[0][0].key, "PrintScreen");
    assert_eq!(c.steps[0][0].label, "Print Screen");
    assert_eq!(
        key_events(&out),
        vec![("PrintScreen".into(), true), ("PrintScreen".into(), false)]
    );
    assert_eq!(h.p.stats().up_only_presses, 1);
    // A modifier key-up with no key-down is ignored.
    let out = h.up("Shift");
    assert!(out.is_empty());
}

#[test]
fn numpad_versus_navigation() {
    let mut h = H::general();
    let out = h.tap("Home");
    assert_eq!(last(&out).steps[0][0].key, "Home");
    h.wait(2000);
    let out = h.tap("NumHome");
    let c = last(&out);
    assert_eq!(c.steps[0][0].key, "NumHome");
    assert_eq!(c.steps[0][0].label, "Num Home");
    h.wait(2000);
    let out = h.tap("NumEnter");
    assert_eq!(last(&out).steps[0][0].key, "NumEnter");
    h.wait(2000);
    let out = h.tap("Enter");
    assert_eq!(last(&out).steps[0][0].key, "Enter");
    // Numpad digits type, so they are masked outside SolidWorks.
    h.wait(2000);
    let out = h.tap("Num7");
    assert_eq!(last(&out).kind, CalloutKind::Masked);
    // In a text run they type digits.
    let mut h = H::new(all_mode());
    h.type_keys(&["Num2", "NumDecimal", "Num5"]);
    assert_eq!(h.callouts()[0].text.as_deref(), Some("2.5"));
}

#[test]
fn stuck_modifier_fixed_by_resync() {
    let mut h = H::general();
    h.down("Ctrl");
    h.tap("S");
    // The Ctrl key-up is lost (for example across a lock screen).
    h.wait(5000);
    assert!(h.p.needs_resync(h.t));
    h.t += 10;
    let out = h.ev(InputEvent::ModifierResync {
        held: vec![],
        t_ms: h.t,
    });
    assert_eq!(key_events(&out), vec![("Ctrl".into(), false)]);
    assert!(!h.p.needs_resync(h.t));
    // Now a plain letter is typing again, not Ctrl + letter.
    let out = h.tap("A");
    assert_eq!(last(&out).kind, CalloutKind::Masked);
    // A resync can also report a modifier the hook missed.
    h.t += 10;
    let out = h.ev(InputEvent::ModifierResync {
        held: vec![(Mod::Shift, Side::Right)],
        t_ms: h.t,
    });
    assert_eq!(key_events(&out), vec![("Shift".into(), true)]);
    assert!(wire(&out)[0].contains(r#""phys":"sc:36""#));
    h.wait(2000);
    let out = h.tap("F3");
    assert_eq!(last(&out).text_form(), "Shift + F3");
    // A resync that confirms the held modifier refreshes it.
    h.wait(2500);
    assert!(h.p.needs_resync(h.t));
    h.ev(InputEvent::ModifierResync {
        held: vec![(Mod::Shift, Side::Right)],
        t_ms: h.t,
    });
    assert!(!h.p.needs_resync(h.t));
}

#[test]
fn stuck_modifier_ages_out() {
    let mut h = H::general();
    h.down("Ctrl");
    // The left Ctrl is held back one tick in case it is AltGr's fake Ctrl.
    assert_eq!(h.p.next_tick_due(), Some(h.t + 1));
    h.wait(1);
    assert_eq!(key_events(&h.tick()), vec![("Ctrl".into(), true)]);
    assert_eq!(h.p.next_tick_due(), Some(h.t - 1 + 10_001));
    h.wait(9_000);
    assert!(key_events(&h.tick()).is_empty());
    h.wait(1_100);
    let out = h.tick();
    assert_eq!(key_events(&out), vec![("Ctrl".into(), false)]);
    let out = h.tap("A");
    assert_eq!(last(&out).kind, CalloutKind::Masked);
    // The lost key-up arriving late is ignored.
    assert!(h.up("Ctrl").is_empty());
    // Auto-repeat keeps a held modifier alive.
    let mut h = H::general();
    h.down("Shift");
    h.step = 33;
    for _ in 0..400 {
        h.down("Shift");
    }
    assert!(
        key_events(&h.tick()).is_empty(),
        "13 s of auto-repeat is still held"
    );
}

#[test]
fn suppression_hides_everything_and_releases_on_exit() {
    let mut h = H::general();
    h.down("Ctrl");
    let enter = h.p.set_suppressed(Some(Suppress::Password));
    // The deferred Ctrl settles first, then everything is released.
    assert_eq!(key_events(&enter), vec![("Ctrl".into(), true)]);
    assert!(matches!(
        enter.last(),
        Some(DisplayEvent::ModifiersReleased { .. })
    ));
    let mut during = Vec::new();
    during.extend(h.tap("S"));
    during.extend(h.up("Ctrl"));
    during.extend(h.type_keys(&["P", "A", "S", "S", "Enter", "F5"]));
    during.extend(h.combo(&["Ctrl"], "V"));
    during.extend(h.button(MouseButton::Left, true, 10, 10));
    during.extend(h.mv(400, 400));
    during.extend(h.button(MouseButton::Left, false, 400, 400));
    during.extend(h.wheel(0, 120));
    h.wait(20_000);
    during.extend(h.tick());
    assert!(during.is_empty(), "nothing while suppressed: {during:?}");
    // Switching the reason keeps it suppressed and silent.
    assert!(h
        .p
        .set_suppressed(Some(Suppress::ExcludedApp("fixture.exe".into())))
        .is_empty());
    // A modifier pressed during suppression and still held on exit is re-lit.
    h.down("Shift");
    let exit = h.p.set_suppressed(None);
    assert!(matches!(exit[0], DisplayEvent::ModifiersReleased { .. }));
    assert_eq!(key_events(&exit), vec![("Shift".into(), true)]);
    for line in wire(&exit) {
        assert!(
            !line.contains("fixture.exe"),
            "the excluded app is never named"
        );
    }
    let out = h.tap("F2");
    assert_eq!(last(&out).text_form(), "Shift + F2");
    // A key pressed during suppression and released after shows nothing.
    let mut h = H::general();
    h.p.set_suppressed(Some(Suppress::Paused)).clear();
    h.down("K");
    h.p.set_suppressed(None).clear();
    assert!(h.up("K").is_empty());
}

#[test]
fn hidden_combos_are_never_shown() {
    let rules = DisplayRules {
        hidden_combos: vec!["Ctrl+Alt+Shift+P".into()],
        ..DisplayRules::default()
    };
    let mut h = H::new(rules);
    let out = h.combo(&["Ctrl", "Alt", "Shift"], "P");
    assert!(callouts_in(&out).is_empty(), "{out:?}");
    assert!(!key_events(&out).iter().any(|(k, _)| k == "P"));
    assert_eq!(h.p.stats().hidden, 1);
    // Other combos with the same key still show.
    let out = h.combo(&["Ctrl"], "P");
    assert_eq!(last(&out).text_form(), "Ctrl + P");
}

#[test]
fn injected_events_follow_the_switch() {
    let injected = |t| {
        InputEvent::Key(KeyInput {
            vk: 0x70,
            phys: PhysKey::new(0x3B, false),
            down: true,
            injected: true,
            t_ms: t,
        })
    };
    let mut h = H::general();
    let out = h.ev(injected(2000));
    assert_eq!(last(&out).text_form(), "F1");
    let rules = DisplayRules {
        show_injected: false,
        ..DisplayRules::default()
    };
    let mut h = H::new(rules);
    assert!(h.ev(injected(2000)).is_empty());
    let click = InputEvent::Button {
        button: MouseButton::Middle,
        down: true,
        x: 0,
        y: 0,
        injected: true,
        t_ms: 2100,
    };
    assert!(h.ev(click).is_empty());
    assert_eq!(h.p.stats().injected_ignored, 2);
}

fn mouse_rules() -> DisplayRules {
    DisplayRules {
        mouse_callouts: true,
        ..DisplayRules::default()
    }
}

#[test]
fn mouse_drags_clicks_and_wheel() {
    let mut h = H::new(mouse_rules());
    h.p.set_monitor(Some(MonitorRect {
        x: 0,
        y: 0,
        width: 1000,
        height: 1000,
    }));
    // Middle drag: created once movement passes 4 px.
    h.button(MouseButton::Middle, true, 500, 500);
    assert!(callouts_in(&h.mv(503, 500)).is_empty(), "within 4 px");
    let out = h.mv(510, 500);
    assert_eq!(last(&out).text_form(), "Middle drag");
    let out = h.button(MouseButton::Middle, false, 600, 520);
    assert!(
        callouts_in(&out).is_empty(),
        "the drag callout already exists"
    );
    match &out[0] {
        DisplayEvent::Button { x, y, .. } => {
            assert!((*x - 0.6).abs() < 1e-6 && (*y - 0.52).abs() < 1e-6)
        }
        other => panic!("{other:?}"),
    }
    // Held modifiers prefix the action.
    h.wait(2000);
    h.down("Ctrl");
    h.button(MouseButton::Middle, true, 500, 500);
    let out = h.mv(540, 500);
    h.button(MouseButton::Middle, false, 540, 500);
    h.up("Ctrl");
    assert_eq!(last(&out).text_form(), "Ctrl + Middle drag");
    // Wheel notches collapse; high resolution deltas accumulate.
    h.wait(2000);
    for _ in 0..5 {
        h.wheel(0, 120);
    }
    let c = h.callouts().pop().unwrap();
    assert_eq!(c.text_form(), "Wheel up x5");
    h.wheel(0, 40);
    h.wheel(0, 40);
    let out = h.wheel(0, 40);
    assert_eq!(last(&out).count, 6);
    let out = h.wheel(0, -360);
    let c = last(&out);
    assert_eq!(c.text_form(), "Wheel down x3");
    let out = h.wheel(120, 0);
    assert_eq!(last(&out).text_form(), "Wheel right");
    // Clicks and double clicks.
    h.wait(2000);
    h.button(MouseButton::Middle, true, 100, 100);
    let out = h.button(MouseButton::Middle, false, 100, 100);
    let first = last(&out);
    assert_eq!(first.text_form(), "Middle click");
    h.button(MouseButton::Middle, true, 102, 101);
    let out = h.button(MouseButton::Middle, false, 102, 101);
    let c = last(&out);
    assert_eq!(c.id, first.id, "the click becomes a double click");
    assert_eq!(c.text_form(), "Middle double click");
    // Too far apart is two clicks.
    h.wait(2000);
    h.button(MouseButton::Right, true, 100, 100);
    h.button(MouseButton::Right, false, 100, 100);
    h.button(MouseButton::Right, true, 200, 100);
    let out = h.button(MouseButton::Right, false, 200, 100);
    assert_eq!(last(&out).text_form(), "Right click x2");
}

#[test]
fn plain_left_clicks_are_quiet_by_default() {
    let mut h = H::new(mouse_rules());
    let mut out = h.button(MouseButton::Left, true, 10, 10);
    out.extend(h.button(MouseButton::Left, false, 10, 10));
    out.extend(h.button(MouseButton::Left, true, 10, 10));
    out.extend(h.mv(200, 200));
    out.extend(h.button(MouseButton::Left, false, 200, 200));
    assert!(callouts_in(&out).is_empty());
    assert_eq!(
        out.iter()
            .filter(|e| matches!(e, DisplayEvent::Button { .. }))
            .count(),
        4,
        "the diagram still sees them"
    );
    h.down("Ctrl");
    h.button(MouseButton::Left, true, 10, 10);
    let out = h.button(MouseButton::Left, false, 10, 10);
    assert_eq!(last(&out).text_form(), "Ctrl + Left click");
    h.up("Ctrl");
    let rules = DisplayRules {
        plain_left_clicks: true,
        ..mouse_rules()
    };
    let mut h = H::new(rules);
    h.button(MouseButton::Left, true, 10, 10);
    let out = h.button(MouseButton::Left, false, 10, 10);
    assert_eq!(last(&out).text_form(), "Left click");
}

#[test]
fn mouse_events_flow_without_callouts() {
    let mut h = H::general();
    h.p.set_monitor(Some(MonitorRect {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1080,
    }));
    let mut out = h.button(MouseButton::Middle, true, -960, 540);
    out.extend(h.mv(-900, 540));
    out.extend(h.button(MouseButton::Middle, false, -900, 540));
    out.extend(h.wheel(0, 120));
    assert!(callouts_in(&out).is_empty());
    assert_eq!(out.len(), 4);
    match &out[0] {
        DisplayEvent::Button {
            x, y, button, down, ..
        } => {
            assert_eq!((*button, *down), (MouseButton::Middle, true));
            assert!((*x - 0.5).abs() < 1e-6 && (*y - 0.5).abs() < 1e-6);
        }
        other => panic!("{other:?}"),
    }
    // Without a monitor, positions are off screen and moves are not sent.
    let mut h = H::general();
    let out = h.mv(5, 5);
    assert!(out.is_empty());
    let out = h.button(MouseButton::Left, true, 5, 5);
    assert!(matches!(out[0], DisplayEvent::Button { x, .. } if x == -1.0));
}

#[test]
fn right_drag_gestures() {
    let mut h = H::new(mouse_rules());
    let gesture = |h: &mut H, dx: i32, dy: i32| {
        h.wait(2000);
        h.button(MouseButton::Right, true, 500, 500);
        let mut out = h.mv(500 + dx / 2, 500 + dy / 2);
        out.extend(h.button(MouseButton::Right, false, 500 + dx, 500 + dy));
        last(&out)
    };
    let cases = [
        ((0, -60), "Gesture up", "RightGestureUp"),
        ((60, -60), "Gesture up-right", "RightGestureUpRight"),
        ((60, 0), "Gesture right", "RightGestureRight"),
        ((60, 60), "Gesture down-right", "RightGestureDownRight"),
        ((0, 60), "Gesture down", "RightGestureDown"),
        ((-60, 60), "Gesture down-left", "RightGestureDownLeft"),
        ((-60, 0), "Gesture left", "RightGestureLeft"),
        ((-60, -60), "Gesture up-left", "RightGestureUpLeft"),
    ];
    for ((dx, dy), text, key) in cases {
        let c = gesture(&mut h, dx, dy);
        assert_eq!(c.text_form(), text);
        assert_eq!(c.steps[0][0].key, key);
        assert_eq!(c.caption, None, "no profile, no caption");
    }
    // Shorter than 40 px: a plain right drag.
    let c = gesture(&mut h, 20, 0);
    assert_eq!(c.text_form(), "Right drag");
    // Too slow: the tick turns it into a right drag, and release adds nothing.
    h.wait(2000);
    h.button(MouseButton::Right, true, 500, 500);
    h.mv(520, 500);
    h.wait(1600);
    let out = h.tick();
    assert_eq!(last(&out).text_form(), "Right drag");
    let out = h.button(MouseButton::Right, false, 600, 500);
    assert!(callouts_in(&out).is_empty());
}

#[test]
fn profile_switch_and_callout_profile_field() {
    let mut h = H::general();
    let out = h.combo(&["Ctrl"], "S");
    assert_eq!(last(&out).profile, None);
    let p = Arc::new(chord_profile());
    h.p.set_profile(p.clone());
    h.p.set_profile(p);
    let out = h.combo(&["Ctrl"], "S");
    let c = last(&out);
    assert_eq!(c.profile.as_deref(), Some("fixture"));
    assert_eq!(c.count, 1, "a profile switch starts fresh");
    h.p.clear_profile();
    assert_eq!(last(&h.combo(&["Ctrl"], "S")).profile, None);
}

#[test]
fn release_all_forgets_held_keys() {
    let mut h = H::general();
    h.down("Ctrl");
    h.down("Up");
    let out = h.p.release_all(h.t + 5);
    assert!(matches!(
        out.as_slice(),
        [DisplayEvent::ModifiersReleased { .. }]
    ));
    assert!(h.up("Up").is_empty(), "the late key-up is not a press");
    let out = h.tap("A");
    assert_eq!(
        last(&out).kind,
        CalloutKind::Masked,
        "Ctrl is no longer held"
    );
}

#[test]
fn caps_lock_changes_text_case() {
    let mut h = H::new(all_mode());
    h.tap("CapsLock");
    h.wait(2000);
    h.type_keys(&["A", "B"]);
    assert_eq!(h.callouts().pop().unwrap().text.as_deref(), Some("AB"));
    let mut h = H::new(all_mode());
    h.p.set_caps_lock(true);
    h.combo(&["Shift"], "A");
    assert_eq!(h.callouts().pop().unwrap().text.as_deref(), Some("a"));
}

#[test]
fn display_events_convert_to_wire_messages() {
    let mut h = H::general();
    let out = h.combo(&["Ctrl"], "S");
    for e in &out {
        let json = e.to_json();
        let back = keystage_core::ServerMsg::from_json(&json).unwrap();
        assert_eq!(back, e.to_server_msg());
    }
    let release = DisplayEvent::ModifiersReleased { t_ms: 5 };
    assert_eq!(release.to_json(), r#"{"t":"release_all","t_ms":5}"#);
}
