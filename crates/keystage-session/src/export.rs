//! Exports (ARCHITECTURE.md section 10): SubRip captions, a Markdown list of
//! the shortcuts used, the session JSON, and YouTube chapter lines. Every
//! export is a pure function of the session; the app writes the text where
//! the user chose.
//!
//! Times in every export are video times from [`RecordingClock`]: OBS timing
//! when the session has it, otherwise the session's own wall time.

use std::collections::HashMap;

use keystage_core::ipc::ExportFormat;
use keystage_core::protocol::CalloutsConfig;
use keystage_core::{Callout, CalloutKind, ChipKind};
use serde::Serialize;

use crate::clock::{Position, RecordingClock};
use crate::session::Session;
use crate::time::{self, HOUR_MS};

/// YouTube's rules for description chapters, from YouTube Help, "Video
/// chapters", <https://support.google.com/youtube/answer/9884579> (read
/// 2026-10-01): "Make sure that the first timestamp you list starts with
/// 00:00. Your video should have at least three timestamps listed in
/// ascending order. The minimum length for video chapters is 10 seconds."
pub const YOUTUBE_MIN_CHAPTERS: usize = 3;
pub const YOUTUBE_MIN_CHAPTER_MS: u64 = 10_000;

/// Name of the first chapter when no marker sits at the very start.
pub const FIRST_CHAPTER_NAME: &str = "Start";
/// A marker this close to the start of the video names the 00:00 chapter.
const START_MARKER_MS: u64 = 1_000;

/// Heading of the Markdown export.
pub const SHORTCUTS_HEADING: &str = "Shortcuts used in this video";

/// Export settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportOptions {
    /// How long a callout stays after its last update (the overlay's
    /// `callouts.linger_ms`).
    pub linger_ms: u64,
    /// Shortest SRT cue. Callouts that start closer together than this share
    /// one cue, one line each.
    pub min_cue_ms: u64,
    /// Most lines in one SRT cue (the newest are kept).
    pub max_cue_lines: usize,
    /// Which recording (0-based) to follow when the session saw several;
    /// `None` follows the last one.
    pub take: Option<usize>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            linger_ms: CalloutsConfig::default().linger_ms,
            min_cue_ms: 1_000,
            max_cue_lines: 3,
            take: None,
        }
    }
}

/// Something the user should know about an export. The text never goes to
/// the log (marker names are the user's words).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExportWarning {
    /// No OBS timing: times count from the start of the session log.
    SessionClock,
    /// The session saw several recordings; `used` is 1-based.
    SeveralTakes { takes: usize, used: usize },
    /// Nothing to export inside the video.
    Empty,
    /// Fewer chapters than YouTube needs.
    FewChapters { count: usize },
    /// A chapter shorter than YouTube's minimum.
    ShortChapter {
        time: String,
        name: String,
        length_ms: u64,
    },
    /// Markers added after the recording stopped were left out.
    MarkersAfterVideo { count: usize },
    /// Two chapters fell in the same second; the later name was kept.
    SameSecond {
        time: String,
        kept: String,
        dropped: String,
    },
    /// The session reached its callout limit.
    Truncated,
}

impl ExportWarning {
    /// Plain UI text (American spelling, no em dashes).
    pub fn message(&self) -> String {
        match self {
            ExportWarning::SessionClock => "OBS timing was not available, so times count from the start of the session log. They match the video only if recording started at the same moment.".to_string(),
            ExportWarning::SeveralTakes { takes, used } => format!(
                "This session holds {takes} recordings. The export follows recording {used}."
            ),
            ExportWarning::Empty => "Nothing was shown during the recording, so there is nothing to export.".to_string(),
            ExportWarning::FewChapters { count } => format!(
                "YouTube shows chapters only when there are at least {YOUTUBE_MIN_CHAPTERS}, starting at 00:00. This list has {count}. Add chapter markers while recording."
            ),
            ExportWarning::ShortChapter { time, name, length_ms } => format!(
                "Chapter \"{name}\" at {time} is {} seconds long. YouTube needs chapters of at least {} seconds.",
                length_ms / 1000,
                YOUTUBE_MIN_CHAPTER_MS / 1000
            ),
            ExportWarning::MarkersAfterVideo { count } => {
                if *count == 1 {
                    "1 chapter marker was added after the recording stopped and is left out.".to_string()
                } else {
                    format!("{count} chapter markers were added after the recording stopped and are left out.")
                }
            }
            ExportWarning::SameSecond { time, kept, dropped } => format!(
                "Two chapters start at {time}. Kept \"{kept}\" and left out \"{dropped}\"."
            ),
            ExportWarning::Truncated => "The session reached its size limit, so later callouts are missing.".to_string(),
        }
    }
}

/// An export's text and what the user should know about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub text: String,
    pub warnings: Vec<ExportWarning>,
}

/// Suggested file name for an export of `session`.
pub fn file_name(session: &Session, format: ExportFormat) -> String {
    let id = &session.id;
    match format {
        ExportFormat::Srt => format!("{id}.srt"),
        ExportFormat::Md => format!("{id}-shortcuts.md"),
        ExportFormat::Json => format!("{id}.json"),
        ExportFormat::Chapters => format!("{id}-chapters.txt"),
    }
}

/// Runs one export.
pub fn export(session: &Session, format: ExportFormat, opts: &ExportOptions) -> Export {
    match format {
        ExportFormat::Srt => srt(session, opts),
        ExportFormat::Md => markdown(session, opts),
        ExportFormat::Json => json(session),
        ExportFormat::Chapters => chapters(session, opts),
    }
}

/// The clock for the chosen take, plus the warnings every timed export shares.
fn clock_for(session: &Session, opts: &ExportOptions) -> (RecordingClock, Vec<ExportWarning>) {
    let mut clock = RecordingClock::new(&session.recording, session.start_ms, session.end_ms);
    if let Some(take) = opts.take {
        clock.select(take);
    }
    let mut warnings = Vec::new();
    if !clock.uses_obs() {
        warnings.push(ExportWarning::SessionClock);
    }
    if clock.takes() > 1 {
        warnings.push(ExportWarning::SeveralTakes {
            takes: clock.takes(),
            used: clock.selected() + 1,
        });
    }
    if session.truncated {
        warnings.push(ExportWarning::Truncated);
    }
    (clock, warnings)
}

/// A callout placed on the video timeline.
struct Placed<'a> {
    callout: &'a Callout,
    start: u64,
    last: u64,
    order: usize,
}

fn placed<'a>(
    session: &'a Session,
    clock: &RecordingClock,
    keep: impl Fn(&Callout) -> bool,
) -> Vec<Placed<'a>> {
    let mut out: Vec<Placed<'a>> = session
        .callouts
        .iter()
        .enumerate()
        .filter(|(_, c)| keep(c))
        .filter_map(|(order, c)| {
            let s = clock.map(c.t_ms);
            let u = clock.map(c.updated_ms.max(c.t_ms));
            if !s.in_video() && !u.in_video() {
                return None;
            }
            Some(Placed {
                callout: c,
                start: s.rec_ms,
                last: u.rec_ms.max(s.rec_ms),
                order,
            })
        })
        .collect();
    out.sort_by_key(|p| (p.start, p.order));
    out
}

/// Replaces control characters (a typed Tab, a stray newline) with spaces so
/// a value can never break a line-based format.
fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

fn nonempty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// The keys of a callout without its repeat badge: `Ctrl + Z`.
fn keys_text(c: &Callout) -> String {
    let mut once = c.clone();
    once.count = 1;
    one_line(&once.text_form())
}

/// `Ctrl + B  Rebuilds the model`, with ` (context)` when the caption has one.
fn caption_line(c: &Callout) -> String {
    let mut line = one_line(&c.text_form());
    if let Some(caption) = nonempty(c.caption.as_deref()) {
        line.push_str("  ");
        line.push_str(&one_line(caption));
        if let Some(ctx) = nonempty(c.context.as_deref()) {
            line.push_str(" (");
            line.push_str(&one_line(ctx));
            line.push(')');
        }
    }
    line
}

/// SubRip captions: one line per callout. Cues are in time order and never
/// overlap; each lasts at least `min_cue_ms` (unless the video ends first).
/// Callouts that start within `min_cue_ms` of a cue's start join it as
/// extra lines. Masked typing is left out (it carries no information).
/// Lines end with CRLF; UTF-8 without a byte order mark.
pub fn srt(session: &Session, opts: &ExportOptions) -> Export {
    let (clock, mut warnings) = clock_for(session, opts);
    let items = placed(session, &clock, |c| c.kind != CalloutKind::Masked);
    let video_end = clock.video_length_ms();
    let min_cue = opts.min_cue_ms.max(1);
    let max_lines = opts.max_cue_lines.max(1);

    struct Cue {
        start: u64,
        end: u64,
        lines: Vec<String>,
    }
    let mut cues: Vec<Cue> = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let start = items[i].start;
        let mut end = (items[i].last + opts.linger_ms).max(start + min_cue);
        let mut lines = vec![caption_line(items[i].callout)];
        let mut j = i + 1;
        while j < items.len() && items[j].start < start + min_cue {
            lines.push(caption_line(items[j].callout));
            end = end.max((items[j].last + opts.linger_ms).max(items[j].start + min_cue));
            j += 1;
        }
        if lines.len() > max_lines {
            lines.drain(..lines.len() - max_lines);
        }
        if let Some(next) = items.get(j) {
            end = end.min(next.start);
        }
        if let Some(ve) = video_end {
            end = end.min(ve.max(start + 1));
        }
        cues.push(Cue { start, end, lines });
        i = j;
    }

    let mut text = String::new();
    for (n, cue) in cues.iter().enumerate() {
        text.push_str(&format!(
            "{}\r\n{} --> {}\r\n",
            n + 1,
            time::srt_time(cue.start),
            time::srt_time(cue.end)
        ));
        for line in &cue.lines {
            text.push_str(line);
            text.push_str("\r\n");
        }
        text.push_str("\r\n");
    }
    if cues.is_empty() {
        warnings.push(ExportWarning::Empty);
    }
    Export { text, warnings }
}

/// True for callouts that belong in the shortcuts list: key combos and
/// chords, plus single keys and mouse actions that have a caption. Typed
/// text, masked typing and bare uncaptioned keys (Enter, arrows) are not
/// shortcuts.
fn is_shortcut(c: &Callout) -> bool {
    let captioned = nonempty(c.caption.as_deref()).is_some();
    match c.kind {
        CalloutKind::Combo | CalloutKind::Chord => {
            captioned
                || c.steps
                    .iter()
                    .flatten()
                    .any(|chip| chip.kind != ChipKind::Modifier)
        }
        CalloutKind::Key | CalloutKind::Mouse => captioned,
        CalloutKind::Text | CalloutKind::Masked => false,
    }
}

/// True when timestamps need hours.
fn needs_hours(clock: &RecordingClock, latest: u64) -> bool {
    latest >= HOUR_MS || clock.video_length_ms().is_some_and(|l| l >= HOUR_MS)
}

/// "Shortcuts used in this video": each unique shortcut once, in order of
/// first use, with its first-use time, command name, context and total
/// count (ASCII `x4`). Plain lines that read well pasted into a YouTube
/// description and render as a Markdown list.
pub fn markdown(session: &Session, opts: &ExportOptions) -> Export {
    let (clock, mut warnings) = clock_for(session, opts);
    let items = placed(session, &clock, is_shortcut);

    struct Entry {
        first: u64,
        keys: String,
        caption: Option<String>,
        context: Option<String>,
        count: u64,
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut by_key: HashMap<(String, Option<String>, Option<String>), usize> = HashMap::new();
    for p in &items {
        let c = p.callout;
        let keys = keys_text(c);
        let caption = nonempty(c.caption.as_deref()).map(one_line);
        let context = caption
            .as_ref()
            .and_then(|_| nonempty(c.context.as_deref()).map(one_line));
        let key = (keys.clone(), caption.clone(), context.clone());
        let uses = u64::from(c.count.max(1));
        match by_key.get(&key) {
            Some(&i) => entries[i].count += uses,
            None => {
                by_key.insert(key, entries.len());
                entries.push(Entry {
                    first: p.start,
                    keys,
                    caption,
                    context,
                    count: uses,
                });
            }
        }
    }

    let latest = entries.iter().map(|e| e.first).max().unwrap_or(0);
    let long = needs_hours(&clock, latest);
    let mut text = format!("{SHORTCUTS_HEADING}\n\n");
    if entries.is_empty() {
        text.push_str("No shortcuts were shown during the recording.\n");
        warnings.push(ExportWarning::Empty);
    }
    for e in &entries {
        text.push_str("- ");
        text.push_str(&time::video_time(e.first, long));
        text.push(' ');
        text.push_str(&e.keys);
        if let Some(caption) = &e.caption {
            text.push_str(": ");
            text.push_str(caption);
            if let Some(ctx) = &e.context {
                text.push_str(" (");
                text.push_str(ctx);
                text.push(')');
            }
        }
        if e.count > 1 {
            text.push_str(&format!(" x{}", e.count));
        }
        text.push('\n');
    }
    Export { text, warnings }
}

/// The session JSON (`keystage.session/1`), pretty printed.
pub fn json(session: &Session) -> Export {
    let mut warnings = Vec::new();
    if session.truncated {
        warnings.push(ExportWarning::Truncated);
    }
    Export {
        text: session.to_json_pretty(),
        warnings,
    }
}

/// YouTube chapter lines: `00:00 Start` (or the name of a marker at the very
/// start), then one line per marker in time order, `MM:SS Name`, or
/// `H:MM:SS Name` from an hour on. Names are the markers' current names, so
/// renames are honored. Warns when YouTube's rules (at least three chapters,
/// each at least 10 seconds) are not met.
pub fn chapters(session: &Session, opts: &ExportOptions) -> Export {
    let (clock, mut warnings) = clock_for(session, opts);
    let mut list: Vec<(u64, String)> = vec![(0, FIRST_CHAPTER_NAME.to_string())];
    let mut after = 0usize;
    for m in &session.markers {
        let mapped = clock.map(m.t_ms);
        if mapped.position == Position::After {
            after += 1;
            continue;
        }
        let name = one_line(&m.name);
        let name = if name.is_empty() {
            FIRST_CHAPTER_NAME.to_string()
        } else {
            name
        };
        if mapped.rec_ms < START_MARKER_MS {
            // A marker at the very start names the first chapter.
            list[0].1 = name;
            continue;
        }
        list.push((mapped.rec_ms, name));
    }
    if after > 0 {
        warnings.push(ExportWarning::MarkersAfterVideo { count: after });
    }
    list.sort_by_key(|(t, _)| *t);

    // Whole seconds, strictly ascending: a later chapter in the same second
    // replaces the earlier one (its name is the newer intent).
    let latest = list.last().map(|(t, _)| *t).unwrap_or(0);
    let long = needs_hours(&clock, latest);
    let mut secs: Vec<(u64, String)> = Vec::with_capacity(list.len());
    for (t, name) in list {
        let s = t / 1000 * 1000;
        match secs.last_mut() {
            Some(prev) if prev.0 == s => {
                warnings.push(ExportWarning::SameSecond {
                    time: time::video_time(s, long),
                    kept: name.clone(),
                    dropped: std::mem::replace(&mut prev.1, name),
                });
            }
            _ => secs.push((s, name)),
        }
    }

    if secs.len() < YOUTUBE_MIN_CHAPTERS {
        warnings.push(ExportWarning::FewChapters { count: secs.len() });
    }
    let video_end = clock.video_length_ms();
    for (i, (t, name)) in secs.iter().enumerate() {
        let next = secs.get(i + 1).map(|(n, _)| *n).or(video_end);
        if let Some(next) = next {
            let length = next.saturating_sub(*t);
            if length < YOUTUBE_MIN_CHAPTER_MS {
                warnings.push(ExportWarning::ShortChapter {
                    time: if i == 0 {
                        "00:00".to_string()
                    } else {
                        time::video_time(*t, long)
                    },
                    name: name.clone(),
                    length_ms: length,
                });
            }
        }
    }

    let mut text = String::new();
    for (i, (t, name)) in secs.iter().enumerate() {
        // YouTube: "the first timestamp you list starts with 00:00".
        let stamp = if i == 0 {
            "00:00".to_string()
        } else {
            time::video_time(*t, long)
        };
        text.push_str(&stamp);
        text.push(' ');
        text.push_str(name);
        text.push('\n');
    }
    Export { text, warnings }
}

impl Session {
    /// Runs one export of this session.
    pub fn export(&self, format: ExportFormat, opts: &ExportOptions) -> Export {
        export(self, format, opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::RecordEvent;
    use crate::session::SessionLog;
    use keystage_core::Chip;

    fn combo(id: u64, t: u64, upd: u64, key: &str, caption: Option<&str>, count: u32) -> Callout {
        Callout {
            id,
            kind: CalloutKind::Combo,
            steps: vec![vec![
                Chip::new("Ctrl", "Ctrl", ChipKind::Modifier),
                Chip::new(key, key, ChipKind::Key),
            ]],
            text: None,
            count,
            caption: caption.map(str::to_string),
            context: None,
            profile: None,
            t_ms: t,
            updated_ms: upd,
        }
    }

    fn with(kind: CalloutKind, mut c: Callout) -> Callout {
        c.kind = kind;
        c
    }

    #[test]
    fn shortcut_filter() {
        let plain = combo(1, 0, 0, "C", None, 1);
        assert!(is_shortcut(&plain));
        let mut tap = plain.clone();
        tap.steps = vec![vec![Chip::new("Ctrl", "Ctrl", ChipKind::Modifier)]];
        assert!(!is_shortcut(&tap), "an uncaptioned modifier tap");
        tap.caption = Some("Fixture".into());
        assert!(is_shortcut(&tap));
        let key = with(CalloutKind::Key, combo(1, 0, 0, "F", None, 1));
        assert!(!is_shortcut(&key));
        let key = with(CalloutKind::Key, combo(1, 0, 0, "F", Some("Fixture"), 1));
        assert!(is_shortcut(&key));
        assert!(!is_shortcut(&with(
            CalloutKind::Text,
            combo(1, 0, 0, "F", Some("x"), 1)
        )));
        assert!(!is_shortcut(&with(
            CalloutKind::Masked,
            combo(1, 0, 0, "F", Some("x"), 1)
        )));
        assert!(!is_shortcut(&with(
            CalloutKind::Mouse,
            combo(1, 0, 0, "F", None, 1)
        )));
    }

    #[test]
    fn one_line_strips_controls() {
        assert_eq!(one_line(" a\tb\nc\r "), "a b c");
    }

    #[test]
    fn srt_merges_close_callouts_and_never_overlaps() {
        let mut log = SessionLog::start("s", 0, 0);
        log.add_callout(&combo(1, 1_000, 1_000, "A", Some("One"), 1));
        log.add_callout(&combo(2, 1_400, 1_400, "B", None, 1));
        log.add_callout(&combo(3, 1_600, 1_600, "C", None, 1));
        log.add_callout(&combo(4, 1_800, 1_800, "D", None, 1));
        log.add_callout(&combo(5, 2_500, 2_500, "E", None, 1));
        log.stop(10_000, 10_000);
        let opts = ExportOptions::default();
        let out = srt(log.session(), &opts);
        let text = out.text.replace("\r\n", "\n");
        assert_eq!(
            text,
            "1\n00:00:01,000 --> 00:00:02,500\nCtrl + B\nCtrl + C\nCtrl + D\n\n2\n00:00:02,500 --> 00:00:04,700\nCtrl + E\n\n"
        );
        assert_eq!(out.warnings, vec![ExportWarning::SessionClock]);
    }

    #[test]
    fn srt_cue_ends_at_video_end() {
        let mut log = SessionLog::start("s", 0, 0);
        log.add_callout(&combo(1, 9_500, 9_500, "A", None, 1));
        log.stop(10_000, 10_000);
        let out = srt(log.session(), &ExportOptions::default());
        assert!(out.text.contains("00:00:09,500 --> 00:00:10,000"));
    }

    #[test]
    fn srt_skips_callouts_outside_the_video() {
        let mut log = SessionLog::start("s", 0, 0);
        log.record_event(1_000, RecordEvent::Started);
        log.add_callout(&combo(1, 500, 500, "A", None, 1));
        log.record_event(5_000, RecordEvent::Paused);
        log.add_callout(&combo(2, 6_000, 6_000, "B", None, 1));
        log.record_event(8_000, RecordEvent::Resumed);
        log.add_callout(&combo(3, 9_000, 9_000, "C", None, 1));
        log.record_event(20_000, RecordEvent::Stopped);
        log.add_callout(&combo(4, 21_000, 21_000, "D", None, 1));
        log.stop(30_000, 30_000);
        let out = srt(log.session(), &ExportOptions::default());
        let text = out.text.replace("\r\n", "\n");
        assert_eq!(text, "1\n00:00:05,000 --> 00:00:07,200\nCtrl + C\n\n");
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn empty_exports_warn() {
        let mut log = SessionLog::start("s", 0, 0);
        log.stop(1_000, 1_000);
        let s = log.session();
        let o = srt(s, &ExportOptions::default());
        assert_eq!(o.text, "");
        assert!(o.warnings.contains(&ExportWarning::Empty));
        let m = markdown(s, &ExportOptions::default());
        assert_eq!(
            m.text,
            "Shortcuts used in this video\n\nNo shortcuts were shown during the recording.\n"
        );
        assert!(m.warnings.contains(&ExportWarning::Empty));
        let c = chapters(s, &ExportOptions::default());
        assert_eq!(c.text, "00:00 Start\n");
        assert!(c
            .warnings
            .contains(&ExportWarning::FewChapters { count: 1 }));
        assert!(c.warnings.iter().any(|w| matches!(
            w,
            ExportWarning::ShortChapter {
                length_ms: 1_000,
                ..
            }
        )));
    }

    #[test]
    fn chapters_same_second_and_start_marker() {
        let mut log = SessionLog::start("s", 0, 0);
        log.add_marker(300, Some("Intro")).unwrap();
        log.add_marker(20_100, Some("First try")).unwrap();
        log.add_marker(20_900, Some("Second try")).unwrap();
        log.add_marker(45_000, Some("Wrap up")).unwrap();
        log.stop(60_000, 60_000);
        let out = chapters(log.session(), &ExportOptions::default());
        assert_eq!(out.text, "00:00 Intro\n00:20 Second try\n00:45 Wrap up\n");
        assert!(out.warnings.contains(&ExportWarning::SameSecond {
            time: "00:20".into(),
            kept: "Second try".into(),
            dropped: "First try".into(),
        }));
        assert!(!out
            .warnings
            .iter()
            .any(|w| matches!(w, ExportWarning::FewChapters { .. })));
    }

    #[test]
    fn chapters_leave_out_markers_after_the_recording() {
        let mut log = SessionLog::start("s", 0, 0);
        log.record_event(0, RecordEvent::Started);
        log.add_marker(15_000, Some("Part")).unwrap();
        log.record_event(30_000, RecordEvent::Stopped);
        log.add_marker(31_000, Some("Too late")).unwrap();
        log.stop(40_000, 40_000);
        let out = chapters(log.session(), &ExportOptions::default());
        assert_eq!(out.text, "00:00 Start\n00:15 Part\n");
        assert!(out
            .warnings
            .contains(&ExportWarning::MarkersAfterVideo { count: 1 }));
    }

    #[test]
    fn warning_messages_read_plainly() {
        let all = [
            ExportWarning::SessionClock,
            ExportWarning::SeveralTakes { takes: 2, used: 2 },
            ExportWarning::Empty,
            ExportWarning::FewChapters { count: 2 },
            ExportWarning::ShortChapter {
                time: "01:00".into(),
                name: "Fixture".into(),
                length_ms: 4_200,
            },
            ExportWarning::MarkersAfterVideo { count: 1 },
            ExportWarning::MarkersAfterVideo { count: 2 },
            ExportWarning::SameSecond {
                time: "00:20".into(),
                kept: "B".into(),
                dropped: "A".into(),
            },
            ExportWarning::Truncated,
        ];
        for w in &all {
            let m = w.message();
            assert!(!m.is_empty());
            assert!(
                !m.contains('\u{2014}') && !m.contains('\u{2013}'),
                "no dashes: {m}"
            );
            assert!(m.ends_with('.'), "{m}");
        }
        assert_eq!(
            all[4].message(),
            "Chapter \"Fixture\" at 01:00 is 4 seconds long. YouTube needs chapters of at least 10 seconds."
        );
        let v = serde_json::to_value(&all[1]).unwrap();
        assert_eq!(v["kind"], "several_takes");
        assert_eq!(v["used"], 2);
    }

    #[test]
    fn file_names() {
        let s = Session::new("2026-10-01T00-00-00Z", 0, 0);
        assert_eq!(file_name(&s, ExportFormat::Srt), "2026-10-01T00-00-00Z.srt");
        assert_eq!(
            file_name(&s, ExportFormat::Md),
            "2026-10-01T00-00-00Z-shortcuts.md"
        );
        assert_eq!(
            file_name(&s, ExportFormat::Json),
            "2026-10-01T00-00-00Z.json"
        );
        assert_eq!(
            file_name(&s, ExportFormat::Chapters),
            "2026-10-01T00-00-00Z-chapters.txt"
        );
    }
}
