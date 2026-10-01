//! Golden-file tests for every export. Captions are synthetic fixture text
//! (never SolidWorks command names). Regenerate after an intended change
//! with `UPDATE_GOLDEN=1 cargo test -p keystage-session --test golden`, then
//! read every changed file before committing it.

use std::fs;
use std::path::PathBuf;

use keystage_core::{Callout, CalloutKind, Chip, ChipKind};
use keystage_session::{
    ExportFormat, ExportOptions, ExportWarning, RecordEvent, Session, SessionLog,
};

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

fn check(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden file {name}; run with UPDATE_GOLDEN=1"));
    // Golden files are stored with LF (the repository normalizes text files).
    assert_eq!(
        actual,
        expected.replace("\r\n", "\n"),
        "export differs from tests/golden/{name}"
    );
}

fn chip(key: &str, label: &str, kind: ChipKind) -> Chip {
    Chip::new(key, label, kind)
}

fn modifier(name: &str) -> Chip {
    chip(name, name, ChipKind::Modifier)
}

fn key(name: &str) -> Chip {
    chip(name, name, ChipKind::Key)
}

#[allow(clippy::too_many_arguments)]
fn callout(
    id: u64,
    kind: CalloutKind,
    steps: Vec<Vec<Chip>>,
    text: Option<&str>,
    count: u32,
    caption: Option<&str>,
    context: Option<&str>,
    t_ms: u64,
    updated_ms: u64,
) -> Callout {
    Callout {
        id,
        kind,
        steps,
        text: text.map(str::to_string),
        count,
        caption: caption.map(str::to_string),
        context: context.map(str::to_string),
        profile: Some("fixture".to_string()),
        t_ms,
        updated_ms,
    }
}

/// 2026-10-01T14:00:00Z.
const STARTED_UNIX_MS: u64 = 1_790_863_200_000;

/// A session recorded with OBS: started 1 s after the log, paused for 10 s,
/// stopped before the log. Callouts cover every kind, one during the pause
/// and one after the recording; markers include one at the very start, one
/// renamed by the user and one after the recording.
fn obs_session() -> Session {
    let mut log = SessionLog::start("2026-10-01T14-00-00Z", STARTED_UNIX_MS, 5_000);
    log.record_event(6_000, RecordEvent::Started);
    log.clock_sample(6_500, 0, false); // output not active yet: ignored
    log.add_marker(6_200, Some("Intro")).unwrap();
    // OBS counted 40 ms of frames before the event arrived.
    log.clock_sample(7_000, 1_040, false);
    let save = |id, t, upd, count| {
        callout(
            id,
            CalloutKind::Combo,
            vec![vec![modifier("Ctrl"), key("S")]],
            None,
            count,
            Some("Fixture save"),
            None,
            t,
            upd,
        )
    };
    log.add_callout(&save(1, 8_000, 8_000, 1));
    let undo = |count, upd| {
        callout(
            2,
            CalloutKind::Combo,
            vec![vec![modifier("Ctrl"), key("Z")]],
            None,
            count,
            Some("Fixture undo"),
            None,
            12_000,
            upd,
        )
    };
    for (n, upd) in [(1, 12_000), (2, 12_500), (3, 13_000), (4, 13_500)] {
        log.add_callout(&undo(n, upd));
    }
    log.add_callout(&callout(
        3,
        CalloutKind::Key,
        vec![vec![key("F")]],
        None,
        1,
        Some("Fixture fit"),
        Some("Graphics area"),
        12_400,
        12_400,
    ));
    log.clock_sample(17_000, 11_030, false);
    log.add_marker(20_000, Some("Sketch")).unwrap();
    log.add_callout(&callout(
        4,
        CalloutKind::Mouse,
        vec![vec![
            modifier("Ctrl"),
            chip("MiddleDrag", "Middle drag", ChipKind::Mouse),
        ]],
        None,
        1,
        Some("Fixture pan"),
        None,
        20_000,
        21_000,
    ));
    log.add_callout(&callout(
        5,
        CalloutKind::Text,
        vec![vec![]],
        Some("25.4"),
        1,
        None,
        None,
        25_000,
        25_600,
    ));
    log.add_callout(&callout(
        5,
        CalloutKind::Text,
        vec![vec![key("Enter")]],
        Some("25.4"),
        1,
        None,
        None,
        25_000,
        26_000,
    ));
    log.clock_sample(27_000, 21_020, false);
    log.add_callout(&callout(
        6,
        CalloutKind::Masked,
        vec![vec![]],
        None,
        1,
        None,
        None,
        30_000,
        31_000,
    ));
    log.add_callout(&callout(
        7,
        CalloutKind::Chord,
        vec![vec![modifier("Alt")], vec![key("F")], vec![key("C")]],
        None,
        1,
        Some("Fixture close"),
        None,
        35_000,
        35_600,
    ));
    log.record_event(40_000, RecordEvent::Paused);
    log.clock_sample(41_000, 34_020, true);
    log.add_callout(&callout(
        8,
        CalloutKind::Combo,
        vec![vec![modifier("Ctrl"), key("P")]],
        None,
        1,
        Some("Fixture during pause"),
        None,
        45_000,
        45_000,
    ));
    log.record_event(50_000, RecordEvent::Resumed);
    log.clock_sample(51_000, 35_020, false);
    log.add_marker(52_000, None).unwrap();
    log.add_callout(&save(9, 60_000, 60_400, 2));
    log.add_callout(&callout(
        10,
        CalloutKind::Mouse,
        vec![vec![chip("WheelUp", "Wheel up", ChipKind::Mouse)]],
        None,
        5,
        Some("Fixture zoom"),
        None,
        70_000,
        71_000,
    ));
    log.add_marker(80_000, None).unwrap();
    log.rename_marker(3, "Wrap up").unwrap();
    log.record_event(95_000, RecordEvent::Stopped);
    log.add_callout(&callout(
        11,
        CalloutKind::Combo,
        vec![vec![modifier("Ctrl"), key("Q")]],
        None,
        1,
        Some("Fixture after stop"),
        None,
        96_000,
        96_000,
    ));
    log.add_marker(97_000, Some("Too late")).unwrap();
    log.stop(100_000, STARTED_UNIX_MS + 95_000);
    log.into_session()
}

/// A session without OBS, longer than an hour, so timestamps need hours.
fn long_session() -> Session {
    let mut log = SessionLog::start("2026-10-01T14-00-00Z", STARTED_UNIX_MS, 0);
    log.add_callout(&callout(
        1,
        CalloutKind::Combo,
        vec![vec![modifier("Ctrl"), modifier("Shift"), key("N")]],
        None,
        1,
        None,
        None,
        65_000,
        65_000,
    ));
    log.add_marker(1_800_000, Some("Halfway")).unwrap();
    log.add_callout(&callout(
        2,
        CalloutKind::Combo,
        vec![vec![modifier("Ctrl"), key("K")]],
        None,
        3,
        Some("Fixture caption"),
        None,
        3_700_000,
        3_701_000,
    ));
    log.add_marker(3_650_000, Some("Last part")).unwrap();
    log.add_marker(3_655_000, Some("Too close")).unwrap();
    log.stop(4_000_000, STARTED_UNIX_MS + 4_000_000);
    log.into_session()
}

fn warnings_text(list: &[(&str, Vec<ExportWarning>)]) -> String {
    let mut out = String::new();
    for (format, warnings) in list {
        out.push_str(&format!("[{format}]\n"));
        for w in warnings {
            out.push_str(&serde_json::to_string(w).unwrap());
            out.push('\n');
            out.push_str(&w.message());
            out.push('\n');
        }
    }
    out
}

fn run_all(prefix: &str, session: &Session) {
    let opts = ExportOptions::default();
    let srt = session.export(ExportFormat::Srt, &opts);
    let md = session.export(ExportFormat::Md, &opts);
    let json = session.export(ExportFormat::Json, &opts);
    let chapters = session.export(ExportFormat::Chapters, &opts);

    // SRT lines end with CRLF; no bare LF anywhere.
    assert!(!srt.text.replace("\r\n", "").contains('\n'));
    check(
        &format!("{prefix}.srt.golden"),
        &srt.text.replace("\r\n", "\n"),
    );
    check(&format!("{prefix}.md.golden"), &md.text);
    check(&format!("{prefix}.json.golden"), &json.text);
    check(&format!("{prefix}.chapters.golden"), &chapters.text);
    check(
        &format!("{prefix}.warnings.golden"),
        &warnings_text(&[
            ("srt", srt.warnings),
            ("md", md.warnings),
            ("json", json.warnings),
            ("chapters", chapters.warnings),
        ]),
    );

    // No export contains an em or en dash.
    for text in [&srt.text, &md.text, &json.text, &chapters.text] {
        assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'));
    }
    // The JSON export reads back to the same session.
    assert_eq!(&Session::from_json(&json.text).unwrap(), session);
}

#[test]
fn golden_obs_session() {
    run_all("obs", &obs_session());
}

#[test]
fn golden_long_session_without_obs() {
    run_all("long", &long_session());
}

#[test]
fn srt_cues_are_ordered_and_never_overlap() {
    for session in [obs_session(), long_session()] {
        let text = session
            .export(ExportFormat::Srt, &ExportOptions::default())
            .text;
        let mut last_end = 0u64;
        let mut n = 0;
        for block in text.split("\r\n\r\n").filter(|b| !b.is_empty()) {
            n += 1;
            let lines: Vec<&str> = block.split("\r\n").collect();
            assert_eq!(lines[0], n.to_string());
            let (a, b) = lines[1].split_once(" --> ").unwrap();
            let parse = |s: &str| -> u64 {
                let (hms, ms) = s.split_once(',').unwrap();
                let p: Vec<u64> = hms.split(':').map(|x| x.parse().unwrap()).collect();
                ((p[0] * 60 + p[1]) * 60 + p[2]) * 1000 + ms.parse::<u64>().unwrap()
            };
            let (start, end) = (parse(a), parse(b));
            assert!(start >= last_end, "cue {n} overlaps the previous one");
            assert!(end > start);
            last_end = end;
            assert!(lines.len() >= 3);
        }
        assert!(n > 0);
    }
}

#[test]
fn choosing_an_earlier_take() {
    let mut log = SessionLog::start("s", STARTED_UNIX_MS, 0);
    log.record_event(1_000, RecordEvent::Started);
    log.add_callout(&callout(
        1,
        CalloutKind::Combo,
        vec![vec![modifier("Ctrl"), key("A")]],
        None,
        1,
        Some("Fixture first take"),
        None,
        3_000,
        3_000,
    ));
    log.record_event(5_000, RecordEvent::Stopped);
    log.record_event(10_000, RecordEvent::Started);
    log.add_callout(&callout(
        2,
        CalloutKind::Combo,
        vec![vec![modifier("Ctrl"), key("B")]],
        None,
        1,
        Some("Fixture second take"),
        None,
        12_000,
        12_000,
    ));
    log.record_event(20_000, RecordEvent::Stopped);
    log.stop(21_000, STARTED_UNIX_MS + 21_000);
    let s = log.into_session();

    let last = s.export(ExportFormat::Md, &ExportOptions::default());
    assert_eq!(
        last.text,
        "Shortcuts used in this video\n\n- 00:02 Ctrl + B: Fixture second take\n"
    );
    assert!(last
        .warnings
        .contains(&ExportWarning::SeveralTakes { takes: 2, used: 2 }));
    let first = s.export(
        ExportFormat::Md,
        &ExportOptions {
            take: Some(0),
            ..ExportOptions::default()
        },
    );
    assert_eq!(
        first.text,
        "Shortcuts used in this video\n\n- 00:02 Ctrl + A: Fixture first take\n"
    );
    assert!(first
        .warnings
        .contains(&ExportWarning::SeveralTakes { takes: 2, used: 1 }));
}
