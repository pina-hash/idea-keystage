//! Copy List parser and `.sldreg` sniffer against the constructed fixtures
//! (`tests/fixtures/sw-shortcuts/`, spec in `.work/research/sldreg.md`).

use keystage_core::import::{
    decode_text, import_bytes, import_text, parse_copy_list, preview, sniff_sldreg, RegData,
    SldregError, TextEncoding,
};
use keystage_core::profile::{Shortcut, USER_IMPORT_SOURCE};
use keystage_core::{ImportFormat, Profile};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/sw-shortcuts/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

const HEADER: &str = "Category\tCommand\tShortcut(s)";
const ROWS: [&str; 6] = [
    "FixtureCategoryA\tFixture Command One\tCtrl+B",
    "FixtureCategoryA\tFixture Command Two\tCtrl + R",
    "FixtureCategoryA\tFixture Command Three\t",
    "FixtureCategoryB\tFixture Command Four\tEnter",
    "FixtureCategoryB\tFixture Command Five\tS",
    "FixtureCategoryB\tFixture Command Six\tCtrl+Tab",
];

/// The expected parse of the six documented rows (spec section A).
fn check_basic(r: &keystage_core::ImportResult, what: &str) {
    assert_eq!(r.format, ImportFormat::CopyList, "{what}");
    assert!(!r.validated, "{what}");
    let got: Vec<(Vec<String>, &str, &str, &str)> = r
        .entries
        .iter()
        .map(|e| {
            (
                e.keys.clone(),
                e.doc_keys.as_str(),
                e.command.as_str(),
                e.category.as_str(),
            )
        })
        .collect();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        got,
        vec![
            (
                s(&["Ctrl", "B"]),
                "Ctrl+B",
                "Fixture Command One",
                "FixtureCategoryA"
            ),
            (
                s(&["Ctrl", "R"]),
                "Ctrl + R",
                "Fixture Command Two",
                "FixtureCategoryA"
            ),
            (
                s(&["Enter"]),
                "Enter",
                "Fixture Command Four",
                "FixtureCategoryB"
            ),
            (s(&["S"]), "S", "Fixture Command Five", "FixtureCategoryB"),
            (
                s(&["Ctrl", "Tab"]),
                "Ctrl+Tab",
                "Fixture Command Six",
                "FixtureCategoryB"
            ),
        ],
        "{what}"
    );
    assert!(r.unparsed.is_empty(), "{what}: {:?}", r.unparsed);
    assert!(r.warnings.is_empty(), "{what}: {:?}", r.warnings);
    assert!(r.entries.iter().all(|e| e.source == USER_IMPORT_SOURCE));
}

#[test]
fn copy_list_fixture_files() {
    for name in [
        "copylist_tab_header_crlf.txt",
        "copylist_tab_noheader_crlf.txt",
        "copylist_tab_header_lf.txt",
        "copylist_tab_header_utf16le_bom.txt",
        "copylist_tab_header_trailing_blank.txt",
    ] {
        let r = import_bytes(&fixture(name), Some(name));
        check_basic(&r, name);
    }
}

#[test]
fn copy_list_variants_built_in_memory() {
    // Built here as well, so the test does not depend on how the fixture
    // files' line endings survive a checkout.
    let lines: Vec<&str> = std::iter::once(HEADER).chain(ROWS).collect();
    let crlf: String = lines.iter().map(|l| format!("{l}\r\n")).collect();
    let lf: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let cr: String = lines.iter().map(|l| format!("{l}\r")).collect();
    let noheader: String = ROWS.iter().map(|l| format!("{l}\r\n")).collect();
    let mut utf16 = vec![0xFF, 0xFE];
    for u in crlf.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
    utf8_bom.extend_from_slice(crlf.as_bytes());
    let semicolons = crlf.replace('\t', ";");
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("crlf", crlf.clone().into_bytes()),
        ("lf", lf.into_bytes()),
        ("cr", cr.into_bytes()),
        ("no header", noheader.into_bytes()),
        ("trailing blank", format!("{crlf}\r\n\r\n").into_bytes()),
        ("utf-16le bom", utf16),
        ("utf-8 bom", utf8_bom),
        ("semicolons", semicolons.into_bytes()),
    ];
    for (what, bytes) in cases {
        check_basic(&import_bytes(&bytes, Some("list.txt")), what);
    }
    // Pasted text takes the same route.
    check_basic(&import_text(&crlf), "pasted");
    // Decoding reports the encoding.
    let d = decode_text(&fixture("copylist_tab_header_utf16le_bom.txt")).unwrap();
    assert_eq!(d.encoding, TextEncoding::Utf16LeBom);
}

#[test]
fn copy_list_multi_and_bad_rows() {
    let r = parse_copy_list(&String::from_utf8(fixture("copylist_multi_and_bad.txt")).unwrap());
    let pairs: Vec<(String, String)> = r
        .entries
        .iter()
        .map(|e| (e.keys.join("+"), e.command.clone()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("Ctrl+B".to_string(), "Fixture Command One".to_string()),
            ("Ctrl+B".to_string(), "Fixture Command Seven".to_string()),
            ("Ctrl+R".to_string(), "Fixture Command Seven".to_string()),
            ("Ctrl+B".to_string(), "Fixture Command Eight".to_string()),
        ]
    );
    // The lowercase entry keeps its raw text but is normalized.
    let eight = r
        .entries
        .iter()
        .find(|e| e.command == "Fixture Command Eight")
        .unwrap();
    assert_eq!(eight.doc_keys, "ctrl+b");
    // Unknown key and two-cell row are kept raw, not shown.
    assert_eq!(r.unparsed.len(), 2, "{:?}", r.unparsed);
    assert!(r.unparsed.iter().any(|u| u.text.contains("Ctrl+???")));
    assert!(r
        .unparsed
        .iter()
        .any(|u| u.text.ends_with("Fixture Command Ten")));
    let w = r.warnings.join("\n");
    assert!(w.contains("unknown key \"???\""), "{w}");
    assert!(w.contains("expected 3 columns, found 2"), "{w}");
    assert!(w.contains("Lowercase letters"), "{w}");
    assert!(
        w.contains("Ctrl + B is assigned to more than one command"),
        "{w}"
    );
    assert!(
        w.contains("Fixture Command One; Fixture Command Seven; Fixture Command Eight"),
        "{w}"
    );
    assert!(
        !w.contains("Ctrl + R is assigned"),
        "Ctrl+R has one command here"
    );
}

#[test]
fn sldreg_empty_binary_and_backup_name() {
    assert_eq!(
        sniff_sldreg(&fixture("empty.sldreg")),
        Err(SldregError::Empty)
    );
    assert_eq!(
        sniff_sldreg(&fixture("opaque_binary.sldreg")),
        Err(SldregError::UnrecognizedFormat)
    );
    assert_eq!(
        sniff_sldreg(&fixture("backup_userid_20261001_120000.sldreg")),
        Err(SldregError::UnrecognizedFormat)
    );
    for name in [
        "empty.sldreg",
        "opaque_binary.sldreg",
        "backup_userid_20261001_120000.sldreg",
    ] {
        let r = import_bytes(&fixture(name), Some(name));
        assert_eq!(r.format, ImportFormat::Sldreg);
        assert!(r.entries.is_empty());
        assert!(!r.validated);
        assert_eq!(r.warnings.len(), 1);
    }
    let r = import_bytes(
        &fixture("opaque_binary.sldreg"),
        Some("opaque_binary.sldreg"),
    );
    assert!(r.warnings[0].contains("Use Copy List"), "{:?}", r.warnings);
}

#[test]
fn sldreg_hypothesis_text_is_tokenized_but_not_trusted() {
    let utf8 = sniff_sldreg(&fixture("hypothesis_regedit5_utf8.sldreg")).unwrap();
    let utf16 = sniff_sldreg(&fixture("hypothesis_regedit5_utf16le_bom.sldreg")).unwrap();
    assert_eq!(
        utf8.file, utf16.file,
        "the encoding does not change the result"
    );
    assert_eq!(utf16.encoding, TextEncoding::Utf16LeBom);
    assert_eq!(utf8.file.keys.len(), 1);
    assert_eq!(utf8.file.keys[0].values.len(), 1);
    assert_eq!(utf8.file.keys[0].values[0].data, RegData::Dword(0x400));
    for name in [
        "hypothesis_regedit5_utf8.sldreg",
        "hypothesis_regedit5_utf16le_bom.sldreg",
    ] {
        let r = import_bytes(&fixture(name), Some(name));
        assert_eq!(r.format, ImportFormat::Sldreg);
        assert!(r.entries.is_empty(), "no shortcuts are guessed");
        assert!(!r.validated);
        assert_eq!(
            r.warnings[0],
            SldregError::NoKnownKeyboardSection.to_string()
        );
        assert!(r.warnings[0].starts_with("Could not read the keyboard section; use Copy List"));
    }
}

#[test]
fn sldreg_truncated_is_malformed() {
    let e = sniff_sldreg(&fixture("hypothesis_truncated.sldreg")).unwrap_err();
    assert!(matches!(e, SldregError::Malformed(_)), "{e:?}");
    let r = import_bytes(&fixture("hypothesis_truncated.sldreg"), Some("x.sldreg"));
    assert!(r.entries.is_empty(), "no partial shortcuts");
}

#[test]
fn preview_marks_added_changed_unchanged() {
    let sourced = Profile {
        id: "fixture".into(),
        shortcuts: vec![
            Shortcut {
                keys: vec!["Ctrl".into(), "B".into()],
                command: "Fixture Command One".into(),
                ..Shortcut::default()
            },
            Shortcut {
                keys: vec!["Enter".into()],
                command: "Fixture sourced enter".into(),
                ..Shortcut::default()
            },
        ],
        ..Profile::default()
    };
    let r = import_bytes(&fixture("copylist_tab_header_lf.txt"), Some("list.txt"));
    let p = preview(&r, &sourced);
    assert_eq!(p.unchanged, 1);
    assert_eq!(p.changed.len(), 1);
    assert_eq!(
        p.changed[0].previous.as_deref(),
        Some("Fixture sourced enter")
    );
    assert_eq!(p.added.len(), 3);
    assert_eq!(p.entries.len(), 5);
    assert!(!p.validated);
    let v = serde_json::to_value(&p).unwrap();
    for field in [
        "format",
        "entries",
        "added",
        "changed",
        "unchanged",
        "warnings",
        "validated",
    ] {
        assert!(v.get(field).is_some(), "{field}");
    }
}

#[test]
fn imported_shortcuts_win_in_the_processor() {
    use keystage_core::{
        builtin, DisplayRules, InputEvent, KeyInput, PhysKey, Processor, UsLegends,
    };
    use std::sync::Arc;
    let r = import_bytes(&fixture("copylist_tab_header_lf.txt"), Some("list.txt"));
    let sw = builtin::solidworks().unwrap();
    let merged = sw.merged_with_user(&r.entries);
    let mut p = Processor::new(DisplayRules::default());
    p.set_profile(Arc::new(merged));
    // Ctrl+B down, B down.
    let ev = |vk, scan, down, t| {
        InputEvent::Key(KeyInput {
            vk,
            phys: PhysKey::new(scan, false),
            down,
            injected: false,
            t_ms: t,
        })
    };
    p.handle(&ev(0xA2, 0x1D, true, 100), &UsLegends);
    let out = p.handle(&ev(0x42, 0x30, true, 120), &UsLegends);
    let c = out.iter().find_map(|e| e.callout()).unwrap();
    assert_eq!(
        c.caption.as_deref(),
        Some("Fixture Command One"),
        "the user's own caption wins"
    );
}
