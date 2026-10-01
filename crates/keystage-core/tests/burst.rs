//! Burst test: 20 keys per second for 10 seconds (200 down/up pairs, mixed
//! combos and plain keys), in shortcuts-only mode and in the SolidWorks
//! profile. Every input must be accounted for: each displayable key-down is
//! reflected in a callout create or update in the same `handle` call, and the
//! counts in the final callouts reconcile exactly with the inputs.

mod common;

use std::sync::Arc;

use common::*;
use keystage_core::{CalloutKind, DisplayEvent, DisplayRules, Processor, Profile, UsLegends};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }
}

#[derive(Debug)]
struct Item {
    mods: Vec<&'static str>,
    key: &'static str,
}

const SPECIAL: [&str; 10] = [
    "F5", "Esc", "Up", "Down", "Enter", "Tab", "Delete", "Home", "F2", "PageDown",
];
const PRINTABLE: [&str; 16] = [
    "A", "B", "H", "J", "K", "L", "P", "U", "1", "2", "3", "7", "9", ".", "-", "0",
];
const COMBO_KEYS: [&str; 7] = ["S", "Z", "Y", "C", "V", "B", "R"];
const ALT_KEYS: [&str; 4] = ["Up", "Down", "Left", "Right"];

fn generate(seed: u64) -> Vec<Item> {
    let mut r = Rng(seed);
    (0..200)
        .map(|_| match r.next() % 20 {
            0..=4 => Item {
                mods: vec!["Ctrl"],
                key: r.pick(&COMBO_KEYS),
            },
            5 => Item {
                mods: vec!["Ctrl", "Shift"],
                key: r.pick(&COMBO_KEYS),
            },
            6 => Item {
                mods: vec!["Alt"],
                key: r.pick(&ALT_KEYS),
            },
            7..=11 => Item {
                mods: vec![],
                key: r.pick(&SPECIAL),
            },
            _ => Item {
                mods: vec![],
                key: r.pick(&PRINTABLE),
            },
        })
        .collect()
}

fn is_printable(k: &str) -> bool {
    !SPECIAL.contains(&k)
}

struct Outcome {
    shown: u64,
    masked: u64,
}

fn run(profile: Option<Profile>, seed: u64, typing_shown: bool) -> Outcome {
    let mut p = Processor::new(DisplayRules::default());
    if let Some(prof) = profile {
        p.set_profile(Arc::new(prof));
    }
    let legends = UsLegends;
    let items = generate(seed);
    let mut all: Vec<DisplayEvent> = Vec::new();
    let mut expected_shown = 0u64;
    let mut expected_masked = 0u64;
    let mut reflected = 0u64;
    let send = |p: &mut Processor, name: &str, down: bool, t: u64, all: &mut Vec<DisplayEvent>| {
        let out = p.handle(&common::key_event(name, down, t), &legends);
        all.extend(out.iter().cloned());
        out
    };
    for (i, item) in items.iter().enumerate() {
        let t = 1000 + 50 * i as u64;
        for (j, m) in item.mods.iter().enumerate() {
            send(&mut p, m, true, t - 20 + j as u64, &mut all);
        }
        let out = send(&mut p, item.key, true, t, &mut all);
        let masked = item.mods.is_empty() && is_printable(item.key) && !typing_shown;
        if masked {
            expected_masked += 1;
            assert!(
                callouts_in(&out)
                    .iter()
                    .all(|c| c.kind == CalloutKind::Masked),
                "a masked key produced a visible callout: {item:?}"
            );
            assert!(
                key_events(&out).is_empty(),
                "a masked key produced a key event: {item:?}"
            );
        } else {
            expected_shown += 1;
            if !callouts_in(&out).is_empty() {
                reflected += 1;
            }
        }
        send(&mut p, item.key, false, t + 15, &mut all);
        for (j, m) in item.mods.iter().enumerate().rev() {
            send(&mut p, m, false, t + 25 + j as u64, &mut all);
        }
    }
    let tail = p.tick(1000 + 50 * 200 + 5000);
    all.extend(tail.iter().cloned());

    // Every displayable key-down produced a callout create or update.
    assert_eq!(
        reflected, expected_shown,
        "seed {seed}: a displayable key-down was dropped"
    );

    // The processor's own counts agree.
    let stats = p.stats();
    assert_eq!(stats.key_presses, 200, "seed {seed}");
    assert_eq!(stats.shown, expected_shown, "seed {seed}");
    assert_eq!(stats.masked, expected_masked, "seed {seed}");
    assert_eq!(stats.repeats, 0);
    assert_eq!(stats.up_only_presses, 0);

    // The final callouts add up to the inputs exactly.
    let finals = finals(&all);
    let mut sum = 0u64;
    for c in &finals {
        let trailing: u64 = c.steps.iter().map(|s| s.len() as u64).sum();
        sum += match c.kind {
            CalloutKind::Combo | CalloutKind::Key => u64::from(c.count),
            CalloutKind::Text => c.text.as_ref().map_or(0, |t| t.chars().count() as u64) + trailing,
            CalloutKind::Masked => trailing,
            CalloutKind::Chord | CalloutKind::Mouse => {
                panic!("seed {seed}: unexpected {:?}", c.kind)
            }
        };
    }
    assert_eq!(
        sum, expected_shown,
        "seed {seed}: callout counts do not reconcile"
    );

    // Key events: one down and one up per shown key; modifiers balanced too.
    let keys = key_events(&all);
    let downs = keys
        .iter()
        .filter(|(k, d)| *d && !matches!(k.as_str(), "Ctrl" | "Shift" | "Alt"))
        .count() as u64;
    let ups = keys
        .iter()
        .filter(|(k, d)| !*d && !matches!(k.as_str(), "Ctrl" | "Shift" | "Alt"))
        .count() as u64;
    assert_eq!(downs, expected_shown);
    assert_eq!(ups, expected_shown);
    for m in ["Ctrl", "Shift", "Alt"] {
        let d = keys.iter().filter(|(k, down)| k == m && *down).count();
        let u = keys.iter().filter(|(k, down)| k == m && !*down).count();
        assert_eq!(d, u, "seed {seed}: {m} key events are balanced");
    }
    let expected_mod_presses: u64 = generate(seed).iter().map(|i| i.mods.len() as u64).sum();
    assert_eq!(stats.modifier_presses, expected_mod_presses);
    assert!(!all
        .iter()
        .any(|e| matches!(e, DisplayEvent::ModifiersReleased { .. })));
    Outcome {
        shown: expected_shown,
        masked: expected_masked,
    }
}

#[test]
fn burst_shortcuts_only() {
    let mut masked_total = 0;
    for seed in [1, 7, 42, 2026, 0xDEAD_BEEF] {
        let o = run(None, seed, false);
        assert_eq!(o.shown + o.masked, 200);
        masked_total += o.masked;
    }
    assert!(masked_total > 0, "the mix includes typing");
}

#[test]
fn burst_solidworks() {
    for seed in [1, 7, 42, 2026, 0xDEAD_BEEF] {
        let o = run(Some(sw_profile()), seed, true);
        assert_eq!(o.shown, 200, "every key shows in the SolidWorks profile");
        assert_eq!(o.masked, 0);
    }
}

#[test]
fn burst_collapses_repeats_and_keeps_ids_stable() {
    // A run of identical combos at 20 per second is one callout.
    let mut h = H::general();
    h.step = 10;
    let mut ids = Vec::new();
    for _ in 0..20 {
        let out = h.combo(&["Ctrl"], "Z");
        ids.push(last(&out).id);
    }
    assert!(ids.iter().all(|i| *i == ids[0]));
    assert_eq!(h.callouts()[0].count, 20);
}
