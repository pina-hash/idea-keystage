# Session file format: `keystage.session/1`

A Session Log is a record of what the overlay showed while you recorded. This page documents its JSON file, which is also what the JSON export writes. Source of truth: `crates/keystage-session/src/session.rs` (`Session`), `src/clock.rs` (`RecordingLog`, `RecordingClock`), `src/export.rs`, the core's `Callout` (`crates/keystage-core/src/event.rs`), and the golden file `crates/keystage-session/tests/golden/obs.json.golden` (a complete example).

## What a session file is

A Session Log exists only when you start one: with the session hotkey (`Ctrl+Alt+Shift+L` by default), the tray, the settings window, or automatically when OBS starts recording, once Connect to OBS is turned on ("Session log with each recording", `obs.auto_session`, is on by default; a log then starts when OBS starts recording and stops when it stops). It holds exactly the `callout` messages the overlay displayed (the final state of each), the chapter markers, and what OBS reported about the recording. Nothing else: no key events, no mouse positions, no app names, nothing typed while the overlay was paused or hidden for a password field or an excluded app.

The app writes the file only when a session stops (with `session.autosave` on, the default) and on Quit for a running session, as `<id>.json` in the sessions folder (`Documents\IDEA Keystage\Sessions` unless `session.folder` names another). The file is pretty-printed UTF-8 JSON without a byte order mark, written to a temporary file and renamed over the target. With autosave off, a stopped session stays in memory for the rest of the run only. The JSON export (`session_export` with `format: "json"`) is the same document.

## Versioning

- `schema` is `"keystage.session/<major>[.<minor>]"`. This version writes `keystage.session/1`.
- A reader accepts `keystage.session/1` and any `keystage.session/1.x`. A different major version, or a missing `schema`, is refused with: the session file uses format "<found>", which this version cannot read.
- Minor versions only add fields. Unknown top-level fields are kept on load and written back on save, so renaming a marker in an older app never drops data written by a newer one.
- Every field except `schema`, `id`, `started`, `started_unix_ms` and `start_ms` may be missing and takes its default.
- A UTF-8 byte order mark at the start is tolerated on load. Files over 128 MiB are refused.

## Top-level fields

| Field             | Type            | Meaning                                                                                                                                        |
| ----------------- | --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `schema`          | string          | `"keystage.session/1"`                                                                                                                         |
| `id`              | string          | Session id, safe as a file name. The default is the UTC start time, `2026-10-01T14-03-22Z`. The app names the file `<id>.json`.                |
| `app`             | object          | `{ "name": "IDEA Keystage", "version": "0.1.0" }`, the app that wrote it                                                                       |
| `started`         | string          | Start, ISO 8601 UTC with milliseconds: `2026-10-01T14:00:00.000Z`                                                                              |
| `started_unix_ms` | integer         | Start, Unix milliseconds                                                                                                                       |
| `ended`           | string or null  | Stop, ISO 8601 UTC; null while the session runs (or if the app ended without stopping it)                                                      |
| `ended_unix_ms`   | integer or null | Stop, Unix milliseconds                                                                                                                        |
| `start_ms`        | integer         | Start on the app's monotonic clock. Every `t_ms` in the file uses this clock, in milliseconds. Its zero is arbitrary; only differences matter. |
| `end_ms`          | integer or null | Stop on the monotonic clock                                                                                                                    |
| `recording`       | object          | What OBS reported (below). Empty lists without OBS.                                                                                            |
| `markers`         | array           | Chapter markers, in time order (below)                                                                                                         |
| `callouts`        | array           | The displayed callouts, final state of each, in the order they first appeared (below)                                                          |
| `truncated`       | boolean         | True when the 100,000 callout limit was reached and later new callouts were dropped                                                            |

## `markers[]`

| Field  | Type    | Meaning                                                                                                                                                                                           |
| ------ | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `t_ms` | integer | Monotonic time the marker was added                                                                                                                                                               |
| `name` | string  | 1 to 100 characters, whitespace collapsed, no control characters. A marker added without a name is `Chapter <n>`. A rename (`session_rename_marker`, by 0-based index in time order) replaces it. |

At most 1,000 markers. Markers always go to the Session Log; when OBS is connected and recording in a format that supports chapters (Hybrid MP4, or Hybrid MOV from OBS 32), the app also adds them to the recording.

## `callouts[]`

Each entry is the wire `callout` object of protocol v1 (`docs/ARCHITECTURE.md` section 5), exactly as the overlay received it in its final state:

| Field        | Type                     | Meaning                                                                                                                                               |
| ------------ | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`         | integer                  | Callout id (the upsert key on the wire)                                                                                                               |
| `kind`       | string                   | `combo`, `key`, `text`, `chord`, `mouse` or `masked`                                                                                                  |
| `steps`      | array of arrays of chips | At least one step. A chip is `{ "key": canonical name or mouse action id, "label": display label in words, "kind": "modifier" or "key" or "mouse" }`. |
| `text`       | string or null           | Typed text of a `text` run (SolidWorks typed values such as `25.4`); null otherwise                                                                   |
| `count`      | integer                  | Repeat count; 1 means no badge                                                                                                                        |
| `caption`    | string or null           | Command name from the active profile (the sourced SolidWorks data or your own import)                                                                 |
| `context`    | string or null           | Where the caption applies, from the profile                                                                                                           |
| `profile`    | string or null           | Profile id (`solidworks`, `general`, or a user profile id)                                                                                            |
| `t_ms`       | integer                  | Monotonic time the callout first appeared                                                                                                             |
| `updated_ms` | integer                  | Monotonic time of its last update                                                                                                                     |

Identity inside a file is the pair `(id, t_ms)`: if the app rebuilt its processor and ids started over, a later callout with a reused id is a separate entry. Masked typing (`kind: "masked"`) is stored as displayed ("Typing", no key identity, `text` null).

## `recording`

```json
{
	"events": [{ "t_ms": 6000, "event": "started" }],
	"samples": [{ "t_ms": 7000, "duration_ms": 1040, "paused": false }]
}
```

- `events[]`: OBS `RecordStateChanged`, derived from `outputState`, stamped with the monotonic time it arrived: `started`, `paused`, `resumed`, `stopped`. At most 10,000.
- `samples[]`: OBS `GetRecordStatus` answers, polled about once a second while recording: at monotonic `t_ms` the recording was `duration_ms` long (`outputDuration`), and `paused` is `outputPaused`. Only samples that change the mapping are stored: the first, one whose paused flag changed, one the previous sample does not predict within 20 ms, and at least one every 15 s. Zero durations (output not active) are not stored. At most 20,000.

### How times map to the video (`RecordingClock`)

- A take is one recording from `started` to `stopped`. A sample with no open take (OBS was already recording when the session learned of it) opens an implicit take that began `duration_ms` before the sample.
- Inside a take, running spans advance one to one with the monotonic clock and paused spans hold video time still. Samples anchor the running spans: between two samples the mapping is interpolated (this absorbs the delay of the `started` event and any drift between OBS media time and the monotonic clock); before the first sample it is extrapolated back; after the last it advances one to one.
- Without OBS data (no events, no samples), video time is the time since `start_ms`, and every export warns that times count from the start of the session log.
- When a session holds several takes, exports follow the last one by default and warn ("This session holds N recordings. The export follows recording M.").

## Exports

All four are made from this file; the app writes the text to the path you choose and shows any warnings.

- **SRT** (`<id>.srt`): one line per callout, `Ctrl + B  Rebuilds the model` (two spaces before the caption; ` (context)` after it when there is one; ASCII `x4` for repeats). Masked typing is left out. So is a callout with nothing to show (a text run emptied with Backspace); a blank line would end the cue. Cues are in time order and never overlap; each lasts at least 1 s unless the video ends first. Callouts starting within 1 s of a cue's start join it as extra lines (the newest 3 kept). A cue lasts until the callout's last update plus the overlay linger time (2,200 ms by default), cut at the next cue. Callouts before the recording, during a pause or after it are left out. Timecodes `HH:MM:SS,mmm`; CRLF line endings; UTF-8 without a byte order mark.
- **Markdown** (`<id>-shortcuts.md`): `Shortcuts used in this video`, then `- MM:SS Keys: Caption (context) xN`, one line per unique shortcut (keys plus caption plus context), in order of first use, with the first-use time and the total count. Key combos and chords always count; single keys and mouse actions only when they have a caption; typed text and masked typing never.
- **JSON** (`<id>.json`): this document.
- **YouTube chapters** (`<id>-chapters.txt`): `00:00 Start` (or the name of a marker within the first second, or of a marker added before the recording started but after the previous one stopped; when several qualify, the latest wins and each replaced name is warned), then each marker `MM:SS Name` (`H:MM:SS` once the video reaches an hour), whole seconds, strictly ascending (a later marker in the same second replaces the earlier one and warns). Markers after the recording stopped are left out with a warning. Markers from an earlier recording in the same session are left out. Warns when there are fewer than 3 chapters or a chapter is shorter than 10 s, per YouTube Help "Video chapters" (https://support.google.com/youtube/answer/9884579, read 2026-10-01: "Make sure that the first timestamp you list starts with 00:00. Your video should have at least three timestamps listed in ascending order. The minimum length for video chapters is 10 seconds.").

## Example

Abbreviated from `crates/keystage-session/tests/golden/obs.json.golden`. The captions there are synthetic test text, not SolidWorks data.

```json
{
	"schema": "keystage.session/1",
	"id": "2026-10-01T14-00-00Z",
	"app": { "name": "IDEA Keystage", "version": "0.1.0" },
	"started": "2026-10-01T14:00:00.000Z",
	"started_unix_ms": 1790863200000,
	"ended": "2026-10-01T14:01:35.000Z",
	"ended_unix_ms": 1790863295000,
	"start_ms": 5000,
	"end_ms": 100000,
	"recording": {
		"events": [
			{ "t_ms": 6000, "event": "started" },
			{ "t_ms": 40000, "event": "paused" },
			{ "t_ms": 50000, "event": "resumed" },
			{ "t_ms": 95000, "event": "stopped" }
		],
		"samples": [
			{ "t_ms": 7000, "duration_ms": 1040, "paused": false },
			{ "t_ms": 27000, "duration_ms": 21020, "paused": false },
			{ "t_ms": 41000, "duration_ms": 34020, "paused": true },
			{ "t_ms": 51000, "duration_ms": 35020, "paused": false }
		]
	},
	"markers": [
		{ "t_ms": 6200, "name": "Intro" },
		{ "t_ms": 20000, "name": "Sketch" }
	],
	"callouts": [
		{
			"id": 2,
			"kind": "combo",
			"steps": [
				[
					{ "key": "Ctrl", "label": "Ctrl", "kind": "modifier" },
					{ "key": "Z", "label": "Z", "kind": "key" }
				]
			],
			"text": null,
			"count": 4,
			"caption": "Fixture undo",
			"context": null,
			"profile": "fixture",
			"t_ms": 12000,
			"updated_ms": 13500
		}
	],
	"truncated": false
}
```

The full golden session exports to these chapters (`obs.chapters.golden`), with the warning "1 chapter marker was added after the recording stopped and is left out.":

```text
00:00 Intro
00:14 Sketch
00:36 Chapter 3
01:04 Wrap up
```
