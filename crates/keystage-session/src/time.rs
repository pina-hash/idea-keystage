//! Time formatting used by the session file and the exports. Pure functions,
//! no clock access: callers pass the numbers in.

/// One hour in milliseconds.
pub const HOUR_MS: u64 = 3_600_000;

/// Days since 1970-01-01 to a proleptic Gregorian `(year, month, day)`.
/// Howard Hinnant's `civil_from_days`, valid for every date a u64 of Unix
/// milliseconds can name.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

struct Utc {
    year: i64,
    month: u32,
    day: u32,
    hour: u64,
    minute: u64,
    second: u64,
    milli: u64,
}

fn utc(unix_ms: u64) -> Utc {
    let secs = unix_ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    Utc {
        year,
        month,
        day,
        hour: rem / 3_600,
        minute: rem % 3_600 / 60,
        second: rem % 60,
        milli: unix_ms % 1000,
    }
}

/// ISO 8601 in UTC with milliseconds: `2026-10-01T14:03:22.123Z`.
pub fn iso_utc(unix_ms: u64) -> String {
    let t = utc(unix_ms);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year, t.month, t.day, t.hour, t.minute, t.second, t.milli
    )
}

/// A file-name-safe session id from the start time, in UTC:
/// `2026-10-01T14-03-22Z`. Sorts by time and has no characters Windows
/// forbids in file names.
pub fn session_id(unix_ms: u64) -> String {
    let t = utc(unix_ms);
    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}Z",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

/// SubRip timecode `HH:MM:SS,mmm` (hours grow past two digits if needed).
pub fn srt_time(ms: u64) -> String {
    let h = ms / HOUR_MS;
    let m = ms % HOUR_MS / 60_000;
    let s = ms % 60_000 / 1000;
    let f = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{f:03}")
}

/// A YouTube-style timestamp, floored to the second: `MM:SS`, or `H:MM:SS`
/// when `long` is set (videos of an hour or more).
pub fn video_time(ms: u64, long: bool) -> String {
    let total = ms / 1000;
    let h = total / 3_600;
    let m = total % 3_600 / 60;
    let s = total % 60;
    if long {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{:02}:{s:02}", m + h * 60)
    }
}

/// `HH:MM:SS.mmm`, the format obs-websocket uses for `outputTimecode`.
pub fn obs_timecode(ms: u64) -> String {
    let h = ms / HOUR_MS;
    let m = ms % HOUR_MS / 60_000;
    let s = ms % 60_000 / 1000;
    let f = ms % 1000;
    format!("{h:02}:{m:02}:{s:02}.{f:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00.000Z");
        // 2000-02-29 (leap day) 12:34:56.789 UTC = 951827696789 ms.
        assert_eq!(iso_utc(951_827_696_789), "2000-02-29T12:34:56.789Z");
        // 2026-10-01T00:00:00Z = 1790812800 s.
        assert_eq!(iso_utc(1_790_812_800_000), "2026-10-01T00:00:00.000Z");
        assert_eq!(iso_utc(1_790_812_800_000 - 1), "2026-09-30T23:59:59.999Z");
        // 2100 is not a leap year: 2100-03-01T00:00:00Z = 4107542400 s.
        assert_eq!(iso_utc(4_107_542_400_000), "2100-03-01T00:00:00.000Z");
        assert_eq!(
            iso_utc(4_107_542_400_000 - 86_400_000),
            "2100-02-28T00:00:00.000Z"
        );
    }

    #[test]
    fn ids_are_file_name_safe() {
        let id = session_id(1_790_812_800_000 + 14 * 3_600_000 + 3 * 60_000 + 22_500);
        assert_eq!(id, "2026-10-01T14-03-22Z");
        assert!(!id.contains([':', '/', '\\', '*', '?', '"', '<', '>', '|']));
    }

    #[test]
    fn srt_timecodes() {
        assert_eq!(srt_time(0), "00:00:00,000");
        assert_eq!(srt_time(61_001), "00:01:01,001");
        assert_eq!(srt_time(3_600_000 + 59 * 60_000 + 59_999), "01:59:59,999");
        assert_eq!(srt_time(100 * HOUR_MS), "100:00:00,000");
    }

    #[test]
    fn video_timestamps() {
        assert_eq!(video_time(0, false), "00:00");
        assert_eq!(video_time(9_999, false), "00:09");
        assert_eq!(video_time(61_000, false), "01:01");
        assert_eq!(video_time(59 * 60_000 + 59_999, false), "59:59");
        assert_eq!(video_time(0, true), "0:00:00");
        assert_eq!(video_time(3_600_000 + 2 * 60_000 + 3_000, true), "1:02:03");
        assert_eq!(video_time(61_000, true), "0:01:01");
    }

    #[test]
    fn obs_timecodes() {
        assert_eq!(obs_timecode(83_450), "00:01:23.450");
        assert_eq!(obs_timecode(0), "00:00:00.000");
    }
}
