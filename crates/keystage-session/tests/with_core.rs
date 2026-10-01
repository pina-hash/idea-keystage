//! Drives the real keystage-core Processor with the sourced SolidWorks
//! profile, logs exactly the callouts it emits, and checks that every
//! caption in the exports is a command from `data/profiles/solidworks.json`.
//! The shortcuts pressed are chosen from the data file by the test, never
//! written into it.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use keystage_core::keys::{self, CHARACTER_SCANS};
use keystage_core::legends::us_scan_to_vk;
use keystage_core::profile::Profile;
use keystage_core::{
    builtin, DisplayEvent, DisplayRules, InputEvent, KeyInput, PhysKey, Processor, UsLegends,
};
use keystage_session::{ExportFormat, ExportOptions, RecordEvent, SessionLog};

/// US `(vk, scan)` for the character keys, derived from the core's tables.
fn character_keys() -> HashMap<String, (u16, u16)> {
    let mut map = HashMap::new();
    for &scan in CHARACTER_SCANS {
        if let Some(vk) = us_scan_to_vk(scan) {
            let name = keys::classify(vk, PhysKey::new(scan, false)).name;
            map.insert(name.into_owned(), (vk, scan));
        }
    }
    map
}

fn modifier_key(name: &str) -> Option<(u16, u16)> {
    match name {
        "Ctrl" => Some((0xA2, 0x1D)),
        "Shift" => Some((0xA0, 0x2A)),
        "Alt" => Some((0xA4, 0x38)),
        _ => None,
    }
}

/// Shortcut entries with no context, one or two modifiers plus one letter
/// key, in file order, distinct by keys.
fn chosen_entries(profile: &Profile, n: usize) -> Vec<Vec<String>> {
    let chars = character_keys();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for s in &profile.shortcuts {
        if !s.context.is_empty() || s.keys.len() < 2 || s.keys.len() > 3 {
            continue;
        }
        let (mods, key) = s.keys.split_at(s.keys.len() - 1);
        let key = &key[0];
        let letter = key.len() == 1 && key.as_bytes()[0].is_ascii_uppercase();
        if !letter || !chars.contains_key(key) {
            continue;
        }
        if !mods.iter().all(|m| modifier_key(m).is_some()) {
            continue;
        }
        if seen.insert(s.keys.clone()) {
            out.push(s.keys.clone());
        }
        if out.len() == n {
            break;
        }
    }
    out
}

struct Driver {
    p: Processor,
    t: u64,
    log: SessionLog,
}

impl Driver {
    fn feed(&mut self, ev: InputEvent) {
        for d in self.p.handle(&ev, &UsLegends) {
            if let DisplayEvent::Callout(c) = &d {
                self.log.add_callout(c);
            }
        }
    }

    fn key(&mut self, vk: u16, scan: u16, down: bool) {
        self.t += 30;
        self.feed(InputEvent::Key(KeyInput {
            vk,
            phys: PhysKey::new(scan, false),
            down,
            injected: false,
            t_ms: self.t,
        }));
    }

    fn press(&mut self, combo: &[String]) {
        let chars = character_keys();
        let (mods, key) = combo.split_at(combo.len() - 1);
        for m in mods {
            let (vk, sc) = modifier_key(m).unwrap();
            self.key(vk, sc, true);
        }
        let (vk, sc) = chars[&key[0]];
        self.key(vk, sc, true);
        self.key(vk, sc, false);
        for m in mods.iter().rev() {
            let (vk, sc) = modifier_key(m).unwrap();
            self.key(vk, sc, false);
        }
        // Let repeat and chord windows close.
        self.t += 2_000;
        let out = self.p.tick(self.t);
        for d in out {
            if let DisplayEvent::Callout(c) = &d {
                self.log.add_callout(c);
            }
        }
    }
}

#[test]
fn exports_take_captions_from_the_data_file() {
    let profile = builtin::solidworks().expect("the SolidWorks data file loads");
    let entries = chosen_entries(&profile, 4);
    assert!(
        entries.len() >= 2,
        "the data file has letter shortcuts with no context"
    );
    let commands_for = |keys: &[String]| -> BTreeSet<String> {
        profile
            .shortcuts
            .iter()
            .filter(|s| s.keys == keys)
            .map(|s| s.command.clone())
            .collect()
    };

    let mut p = Processor::new(DisplayRules::default());
    p.set_profile(Arc::new(profile.clone()));
    let mut d = Driver {
        p,
        t: 10_000,
        log: SessionLog::start("with-core", 1_790_863_200_000, 10_000),
    };
    d.log.record_event(10_500, RecordEvent::Started);
    d.t = 11_000;
    for keys in &entries {
        d.press(keys);
    }
    // The first shortcut again: a second callout, counted twice in Markdown.
    d.press(&entries[0]);
    d.log.add_marker(d.t, Some("Second half")).unwrap();
    d.t += 15_000;
    d.log.record_event(d.t, RecordEvent::Stopped);
    d.log.stop(d.t + 1_000, 1_790_863_300_000);
    let session = d.log.into_session();
    assert_eq!(session.callouts.len(), entries.len() + 1);

    let md = session.export(ExportFormat::Md, &ExportOptions::default());
    let lines: Vec<&str> = md.text.lines().skip(2).collect();
    assert_eq!(lines.len(), entries.len(), "{}", md.text);
    for (line, keys) in lines.iter().zip(&entries) {
        let caption = line
            .split_once(": ")
            .map(|(_, rest)| rest.trim_end_matches(" x2"))
            .unwrap_or_else(|| panic!("no caption in {line}"));
        assert!(
            commands_for(keys).contains(caption),
            "{caption:?} is not a command for {keys:?} in the data file"
        );
    }
    assert!(lines[0].ends_with(" x2"), "{}", lines[0]);

    let srt = session.export(ExportFormat::Srt, &ExportOptions::default());
    let captions: Vec<&str> = srt
        .text
        .split("\r\n")
        .filter_map(|l| l.split_once("  ").map(|(_, c)| c))
        .collect();
    assert_eq!(captions.len(), entries.len() + 1);
    let all: BTreeSet<String> = profile
        .shortcuts
        .iter()
        .map(|s| s.command.clone())
        .collect();
    for c in captions {
        assert!(all.contains(c), "{c:?} is not in the data file");
    }

    let chapters = session.export(ExportFormat::Chapters, &ExportOptions::default());
    assert!(chapters.text.starts_with("00:00 Start\n"));
    assert!(chapters.text.contains(" Second half\n"));
}
