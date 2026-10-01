# IDEA Keystage architecture contract

This file is the contract every surface is built against. When code and this file disagree, fix one of them in the same commit. American spelling, no em dashes.

## 1. Shape of the system

```
 Windows (src-tauri/src/platform/windows)            Rust app (src-tauri/src/app)                 Hosts
 ┌──────────────────────────────┐   lock-free    ┌──────────────────────────────────┐
 │ hook thread: WH_KEYBOARD_LL,  │ ─ArrayQueue──▶ │ pipeline thread                    │ ─Tauri event "keystage://overlay"─▶ Host A: native overlay window
 │ WH_MOUSE_LL, own message loop │   + unpark     │  keystage_core::Processor          │                                     (overlay.html?host=native)
 └──────────────────────────────┘                │  privacy gate, profile switch      │ ─Hub (tokio broadcast)─▶ keystage-server ─ws─▶ Host B: OBS Browser Sources
 ┌──────────────────────────────┐   crossbeam    │  mouse-move coalescing (<=125 Hz)  │                                     (http://127.0.0.1:47820/overlay/<route>)
 │ watcher thread: foreground,   │ ─channel────▶ │  session log, sound, status        │ ─Tauri event─▶ settings window live preview
 │ focus/password, unlock, layout│                └──────────────────────────────────┘
 └──────────────────────────────┘
```

- One renderer (`overlay.html`, Svelte 5) is used by both hosts and by the settings preview. It is fed by a `Transport`: Tauri events (`host=native` or `host=preview`), WebSocket (OBS and any browser), or the built-in demo script (`demo=1`).
- All decisions about WHAT is shown (combos, repeats, chords, text runs, masking, captions, suppression) are made once, in Rust, in `keystage-core`. The renderer decides only HOW it looks. The Session Log records exactly the `callout` messages the renderer was sent.
- The watcher publishes a password hold word (an epoch plus `Open | Hold | Drop`) in a static atomic; the hook procedures stamp it into every record with one atomic load, and the input receiver decides each record by it (section 8).
- A panic in a pipeline pass does not end the thread: the core is rebuilt from the last state the shell and the watcher sent, with the foreground unidentified until the watcher reports it again (fail closed), and every host gets `release_all`. After three such panics the pipeline stops showing input (the hook queue is still drained and dropped), and `Status.notices` and the tray say so ("Stopped after an error").

## 2. Repository layout and ownership

| Path                                                                                                                                                                                                                           | Owner surface                  | Notes                                                                                                                                                        |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `Cargo.toml` (workspace)                                                                                                                                                                                                       | scaffold                       | members: the four crates under `crates/` plus `src-tauri`; `default-members` lists the four crates only, so `cargo test` on Linux never builds the Tauri app |
| `crates/keystage-core/`                                                                                                                                                                                                        | (a) input core                 | pure Rust: no IO, no threads, no async, no OS calls. Builds and tests on Linux                                                                               |
| `crates/keystage-session/`                                                                                                                                                                                                     | (f) server, OBS, exports       | Session Log, chapter markers, timecode mapping, exports (srt, md, json, chapters). Pure Rust, depends on keystage-core                                       |
| `crates/keystage-server/`                                                                                                                                                                                                      | (f)                            | axum server, WebSocket hub, routes, origin and host checks, `demo` example binary                                                                            |
| `crates/keystage-obs/`                                                                                                                                                                                                         | (f)                            | obs-websocket 5 client, auth, record state and timecode                                                                                                      |
| `src-tauri/src/platform/**`                                                                                                                                                                                                    | (b) Windows layer              | hooks, watcher, UIA password check, foreground process, legends, monitors, window styles, DPAPI, session notifications; `platform/stub.rs` for non-Windows   |
| `src-tauri/src/app/**`, `src-tauri/src/main.rs`, `lib.rs`, `tauri.conf.json`, `capabilities/`, `icons/`, `build.rs`                                                                                                            | (e) app shell                  | pipeline wiring, tray, hotkeys, IPC commands, settings store, profiles, sound playback, overlay window management                                            |
| `src/overlay/**`, `src/lib/renderer/Callouts*.svelte`, `Keycap.svelte`, `src/lib/renderer/transport.ts`, `store.svelte.ts`, `src/lib/themes/**`, `src/lib/styles/**`, `src/lib/protocol.ts`, `src/lib/demo/**`, `overlay.html` | (c) renderer and themes        |                                                                                                                                                              |
| `src/lib/renderer/keyboard/**`, `MouseDiagram.svelte`, `CursorFx.svelte`                                                                                                                                                       | (d) keyboard and mouse visuals |                                                                                                                                                              |
| `src/settings/**`, `settings.html`                                                                                                                                                                                             | (e) app shell                  | settings window, first run, live preview                                                                                                                     |
| `data/profiles/*.json`                                                                                                                                                                                                         | Phase 1 research               | sourced data only (see section 9)                                                                                                                            |
| `scripts/**`, `.github/**`, `tests/unit/**`, `tests/e2e/**`                                                                                                                                                                    | (g) CI and release             | screenshots, perf harness, workflows                                                                                                                         |
| `docs/**`, `README.md`, `CLAUDE.md`, `AGENTS.md`                                                                                                                                                                               | docs owner                     | writes last; other surfaces send notes, not edits                                                                                                            |

A surface may read anything. It writes only the files it owns. If it needs a change in another surface's file, it says so in its report.

## 3. Event model (keystage-core)

Times are `t_ms: u64`, milliseconds on a monotonic clock supplied by the caller (the Windows layer uses the hook's `time` field rebased; tests use literal numbers).

```rust
pub struct PhysKey { pub scan: u16, pub extended: bool }        // wire id: "sc:1E", extended "sc:E01D"
pub enum Side { Left, Right }
pub enum Mod { Ctrl, Shift, Alt, Win }                           // AltGr is reported as its own key name "AltGr"
pub struct KeyInput { pub vk: u16, pub phys: PhysKey, pub down: bool, pub injected: bool, pub t_ms: u64 }
pub enum MouseButton { Left, Right, Middle, X1, X2 }
pub enum InputEvent {
    Key(KeyInput),
    Button { button: MouseButton, down: bool, x: i32, y: i32, injected: bool, t_ms: u64 },
    Wheel { dx: i32, dy: i32, x: i32, y: i32, t_ms: u64 },       // raw WHEEL_DELTA units (+120 = one notch up / right)
    Move { x: i32, y: i32, t_ms: u64 },                          // physical screen pixels (per-monitor DPI aware v2)
    ModifierResync { held: Vec<(Mod, Side)>, t_ms: u64 },        // from GetAsyncKeyState on foreground change, unlock, resume
}
```

- The resync after a foreground change and after each hook install arrives from `InputReceiver::drain_into`, after the hook records already queued; a modifier with a hook event in the last 100 ms keeps the stream's state, because the hook runs before the asynchronous key state updates.

- Auto-repeat is detected by the core: a key-down for a key already down is a repeat. The LL hook has no repeat flag.
- Left and right modifiers are tracked separately. Display uses the generic name (`Ctrl`) unless `rules.show_modifier_side` is on (`Left Ctrl`).
- AltGr: a left Ctrl key-down whose scan code is `0x21D` (or a left Ctrl that arrives with the same timestamp immediately before a right Alt) is the fake Ctrl Windows sends for AltGr; it is swallowed and the right Alt is named `AltGr`.
- A key-up with no matching key-down (Print Screen does this) is treated as a press.
- Numpad vs navigation: VK plus the extended flag decides. Non-extended VK_HOME/END/PRIOR/NEXT/arrows/INSERT/DELETE/CLEAR coming from numpad scan codes are `NumHome` ... `NumClear`; extended VK_RETURN is `NumEnter`.

### Canonical key names

Used in profiles, matching, the wire protocol (`key` fields) and exports:
`Ctrl Shift Alt Win AltGr`, `A`..`Z`, `0`..`9`, `F1`..`F24`, `Space Enter Esc Tab Backspace Delete Insert Home End PageUp PageDown Up Down Left Right CapsLock PrintScreen ScrollLock Pause Menu NumLock`, numpad `Num0`..`Num9 NumAdd NumSubtract NumMultiply NumDivide NumDecimal NumEnter NumHome NumEnd NumPageUp NumPageDown NumUp NumDown NumLeft NumRight NumInsert NumDelete NumClear`, punctuation by US legend `` ` - = [ ] \ ; ' , . / ``, media `VolumeUp VolumeDown VolumeMute MediaPlayPause MediaNext MediaPrev MediaStop BrowserBack BrowserForward BrowserRefresh BrowserHome BrowserSearch LaunchMail LaunchApp1 LaunchApp2`, the 102nd key `IntlBackslash`, and `VK_xx` (hex) for anything else.

The DISPLAY label of a character key comes from the active Windows layout through a `LegendProvider` (`fn legend(&self, vk: u16, phys: PhysKey) -> Option<String>`) the platform layer implements. Tests use a US provider. Profile matching always uses canonical names, which come from the VK, so on AZERTY `Ctrl + Z` still matches the Undo entry for the VK_Z key.

## 4. Processor (keystage-core)

```rust
let mut p = Processor::new(rules: DisplayRules);
p.set_profile(Arc<Profile>);                 // captions + rule overrides; clear_profile() for none
let ev = p.set_suppressed(Option<Suppress>);  // Password | ExcludedApp(String) | Paused; #[must_use], forward the events
p.set_monitor(Option<MonitorRect>);           // for x,y normalization
let out: Vec<DisplayEvent> = p.handle(&InputEvent, &dyn LegendProvider);
let out2 = p.tick(now_ms);                    // closes runs/chords whose windows expired, ages modifier state
// also: next_tick_due(), needs_resync(now), release_all(t_ms), set_caps_lock(bool), stats()
```

`DisplayRules` fields beyond the ones named below, with their defaults: `chord_max_steps` 4, `plain_left_clicks` false, `drag_threshold_px` 4, `double_click_ms` 500, `double_click_px` 4, `gesture_min_px` 40, `gesture_max_ms` 1500, `modifier_verify_ms` 2000, `modifier_stale_ms` 10000; the typing mode field is `typing` (`"shortcuts_only" | "all"`). The left Ctrl key-down is held back until the next event or the next `tick` past its timestamp, so the AltGr fallback never lights a fake Ctrl.

- While suppressed, `handle` returns nothing except the modifier bookkeeping it needs internally. Nothing reaches a host, the log or the sound player. Leaving suppression emits `ModifiersReleased` so no stale key stays lit.
- Combo: a non-modifier key down while any modifier is held makes one callout: chips `Ctrl`, `Shift`, `S`, text form `Ctrl + Shift + S` (modifier order Ctrl, Shift, Alt, AltGr, Win).
- Repeat collapsing: the same combo (or same single key) again within `repeat_window_ms` (default 1000), or an auto-repeat, updates the SAME callout id with `count += 1`. Text form `Ctrl + Z x4`.
- Chord grouping: a solo Alt tap followed by single keys within `chord_window_ms` (default 900) becomes one callout with several steps (`Alt, F, C`). Profiles may also declare chord leaders (`"chords": [["Ctrl","K"]]`), after which the next combo within the window joins the same callout as a second step. (Examples in this file use entries that exist in the sourced data; the often-quoted `Ctrl + 8` "Normal To" is documented only by a 2025 screenshot and not at all for 2026, see DECISIONS item 3.) An Alt-tap chord stops at `chord_max_steps` (Alt plus three keys), so a quick Alt tap cannot reveal a long run of typed letters. The first combo of any multi-step profile entry is also a leader.
- Text runs: when typing is shown, consecutive printable characters (no modifier except Shift, or AltGr) within `text_run_gap_ms` (default 1500) extend one `Text` callout. Backspace removes the last character of an open run; Enter and Tab close it and appear as a trailing chip.
- Masking (`TypingMode::ShortcutsOnly`, the default outside SolidWorks): printable keys with no modifier other than Shift are masked. A masked keystroke produces at most one `Masked` callout per run ("Typing") with no key identity, and no `key` message, so the keyboard visual never reveals text. Modifier combos and special keys (F-keys, Esc, Enter, Tab, Backspace, Delete, arrows, navigation, PrintScreen, media) are shown.
- SolidWorks profile (`TypingMode::All`): bare single keys are shown; a single key that matches a profile shortcut is a `Key` callout with its caption, never merged into a text run. Typed values (digits, `.`, `-`, letters that are not shortcuts) form text runs while `typed_values` is on; when it is off they are masked exactly as above, but profile single-key shortcuts still show.
- Mouse callouts (only when `rules.mouse_callouts`, on in the SolidWorks profile): button down plus movement beyond 4 px while held is a drag (`Middle drag`); two clicks of the same button within 500 ms and 4 px is a double click; a wheel notch is `Wheel up` / `Wheel down` and repeats collapse (`x5`); held modifiers are prefixed (`Ctrl + Middle drag`). Captions come from the profile `mouse` table. A right-button drag of at least 40 px whose release happens within 1.5 s is classified into one of 8 directions as a `gesture` candidate; it is captioned only if the profile has a sourced gesture entry for that direction. Plain left clicks, double clicks and drags with no modifier make no callout unless `plain_left_clicks` is on, because every SolidWorks selection would otherwise be a callout.
- Solo modifier taps make callouts (`Ctrl`, `Ctrl + Shift`); only an Alt tap opens a chord. Backspace inside an open masked run is absorbed into the run. A masked run is one callout id, re-sent at most once a second while typing continues, and never carries count, text or chips. AltGr counts like Shift for masking and text runs.
- In the SolidWorks profile with typed values on, value keys (digits, numpad digits and operators, `.`, `,`, `-`) always join text runs, even though the data has a bare `-` entry.
- The app's own global hotkeys are passed in `rules.hidden_combos` and never shown.
- Injected events are shown unless `rules.show_injected` is off.

```rust
pub enum DisplayEvent {
    Callout(Callout),                                           // upsert by id
    Key { phys: String, key: String, down: bool, modifier: Option<Mod>, t_ms: u64 },
    Button { button: MouseButton, down: bool, x: f32, y: f32, t_ms: u64 },   // x,y normalized to the chosen monitor (0..1, may fall outside)
    Wheel { dx: i32, dy: i32, x: f32, y: f32, t_ms: u64 },
    Move { x: f32, y: f32, t_ms: u64 },
    ModifiersReleased { t_ms: u64 },
}
pub struct Callout {
    pub id: u64, pub kind: CalloutKind,            // Combo | Key | Text | Chord | Mouse | Masked
    pub steps: Vec<Vec<Chip>>,                     // >= 1 step; a plain combo has one step
    pub text: Option<String>,                      // Text runs only
    pub count: u32,                                // 1 = no badge
    pub caption: Option<String>, pub context: Option<String>,   // from the profile
    pub profile: Option<String>,
    pub t_ms: u64, pub updated_ms: u64,
}
pub struct Chip { pub key: String /* canonical */, pub label: String /* display */, pub kind: ChipKind /* Modifier | Key | Mouse */ }
impl Callout { pub fn text_form(&self) -> String }   // "Ctrl + Shift + S", "Alt, F, C", "Ctrl + Z x4", "25.4 Enter"
```

## 5. Wire protocol v1 (WebSocket text frames and Tauri event payloads, identical JSON)

Server to client. Every message has `"t"`.

```jsonc
{ "t": "hello", "v": 1, "app": "IDEA Keystage", "version": "0.1.0", "config": OverlayConfig, "state": AppState, "legends": Legends | null }
{ "t": "config", "config": OverlayConfig }
{ "t": "state", "state": AppState }
{ "t": "legends", "layout": "00000409", "legends": { "sc:1E": "A", "sc:27": ";" } }
{ "t": "callout", "callout": { "id": 7, "kind": "combo", "steps": [[{"key":"Ctrl","label":"Ctrl","kind":"modifier"},{"key":"B","label":"B","kind":"key"}]], "text": null, "count": 1, "caption": "Rebuilds the model", "context": null, "profile": "solidworks", "t_ms": 1234, "updated_ms": 1234 } }
{ "t": "key", "phys": "sc:1D", "key": "Ctrl", "down": true, "modifier": "ctrl", "t_ms": 1234 }
{ "t": "button", "button": "middle", "down": true, "x": 0.52, "y": 0.47, "t_ms": 1234 }
{ "t": "wheel", "dx": 0, "dy": -120, "x": 0.52, "y": 0.47, "t_ms": 1234 }
{ "t": "move", "x": 0.52, "y": 0.47, "t_ms": 1234 }
{ "t": "release_all", "t_ms": 1234 }
{ "t": "clear" }
```

Client to server: `{ "t": "hello", "route": "keys", "host": "obs" | "browser" | "native" | "preview" }` (informational), `{ "t": "ping" }` answered by `{ "t": "pong" }`.

```ts
type AppState = {
	paused: boolean;
	suppressed: null | 'password' | 'excluded'; // never names the excluded app on the wire
	profile: { id: string; name: string } | null; // e.g. { id: "solidworks", name: "SolidWorks" }
	solidworks_mode: 'auto' | 'on' | 'off';
	keyboard_visible: boolean;
	native_visible: boolean; // false: the renderer in Host A (host=native) draws nothing (OBS sources in use, or hidden)
	obs_clients: number; // connected OBS/browser sources
	recording: null | { active: boolean; paused: boolean; timecode: string }; // from OBS, null if not connected
	session: null | { active: boolean; started_ms: number; markers: number };
};
type Anchor =
	| 'top-left'
	| 'top'
	| 'top-right'
	| 'left'
	| 'center'
	| 'right'
	| 'bottom-left'
	| 'bottom'
	| 'bottom-right';
type OverlayConfig = {
	theme: string; // "idea" | "studio" | "graphite" | "blueprint" | "carbon" | "glass" | "custom"
	custom: { accent: string; face: string; ink: string; glow: string } | null;
	scale: number; // 1 = sized for 1080p
	font: 'rajdhani' | 'orbitron' | 'share-tech-mono';
	callouts: {
		enabled: boolean;
		anchor: Anchor;
		offset_x: number;
		offset_y: number;
		max_stack: number;
		linger_ms: number;
		fade_ms: number;
		size: 's' | 'm' | 'l' | 'xl';
		direction: 'up' | 'down';
		captions: boolean;
		symbols: boolean;
	}; // symbols: arrows and Enter as glyphs instead of words
	keyboard: {
		enabled: boolean;
		layout: 'full' | 'tkl' | '75' | '60' | 'laptop' | 'left';
		anchor: Anchor;
		offset_x: number;
		offset_y: number;
		decay_ms: number;
		mouse: boolean;
		size: 's' | 'm' | 'l';
	};
	mouse: { enabled: boolean; anchor: Anchor; offset_x: number; offset_y: number };
	cursor: {
		enabled: boolean;
		style: 'ring' | 'spotlight' | 'dot';
		size: number;
		color: string;
		opacity: number;
		ripples: boolean;
		trail: boolean;
		wheel: boolean;
	};
	indicators: { paused: boolean; suppressed: boolean };
};
```

Query parameters (Host B, any route) override the config for that source only: `theme`, `scale` (0.25 to 4), `anchor` (callouts on `keys` and `all`, keyboard on `keyboard`, diagram on `mouse`; `cursor` ignores it), `x`, `y` (offsets in px at scale 1, inward from the anchored edge, -4000 to 4000), `kb` (keyboard layout), `stack` (1 to 12), `size` (callout size `s|m|l|xl` on `keys` and `all`, keyboard size `s|m|l` on `keyboard`), `demo=1` (also `demo=solidworks`, `demo=general`, `demo=states`), `status=1` (show connection status even inside OBS). A URL anchor also sets the callout direction (top anchors stack downward). A source is fully configured by its URL. A dedicated route (`keys`, `keyboard`, `mouse`, `cursor`) draws its piece whatever the `enabled` switches say; `all` follows each switch; the keyboard hotkey (`keyboard_visible`) hides the keyboard everywhere.

The `Legends` type in `hello` is `{ layout: string; legends: Record<string, string> }`, the `legends` message without `t`. `Status` and `AppState` never name an excluded app. Chip `label` is always words (`Page Up`, `Num 7`); the renderer draws symbol glyphs from the canonical `key` when `callouts.symbols` is on. Mouse chip keys are part, action, direction: `LeftClick`, `MiddleDoubleClick`, `MiddleDrag`, `WheelUp`, `RightGestureUpRight`.

Pieces are placed at their anchors. On a page that draws several (`all`, Host A, the preview), the callouts move inward, clear of the keyboard and the mouse diagram, when one of them covers the row the stack grows from (a top or bottom anchor), keeping 16 px at scale 1 between them.

## 6. Hosts and routes

- Server: `keystage-server`, bound to `127.0.0.1` only, default port `47820`, configurable. Routes:
  - `GET /overlay/{keys|keyboard|mouse|cursor|all}` returns `overlay.html` (the SPA reads `location.pathname`).
  - `GET /assets/*`, `/fonts/*`, `/sounds/*` static files from the built front end (`dist/`). The Tauri app serves them from its own embedded assets (`AssetResolver`); the demo binary and tests read `dist/` from disk. The server crate takes an `Arc<dyn Assets>`.
  - `GET /ws` WebSocket. Accepted only when the `Host` header is `127.0.0.1:<port>` or `localhost:<port>` and the `Origin` header is absent or one of `http://127.0.0.1:<port>`, `http://localhost:<port>`. Anything else gets 403. This blocks a web page in an ordinary browser from reading keystrokes through the socket, and blocks DNS rebinding.
  - `GET /api/health` returns `{"ok":true,"app":"IDEA Keystage","version":"0.1.0","protocol":1}`.
  - No CORS headers, no other routes. Wrong methods on known routes are 404 too. Every response carries `X-Content-Type-Options: nosniff`; overlay pages also carry `Referrer-Policy: no-referrer` and a CSP (`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src 'self' ws://127.0.0.1:<port> ws://localhost:<port>; object-src 'none'; base-uri 'none'; form-action 'none'`). Vite emits fonts under `/assets/`, so `/fonts/*` and `/sounds/*` serve nothing in this build.
  - At most 64 WebSocket clients (`max_clients`); more get 503. Client frames are limited to 4 KiB. The server pings every 15 s and drops a client silent for 45 s or one that cannot take a message for 10 s; a client that falls behind gets a fresh `hello` and then the newest messages. Accepted sockets use `TCP_NODELAY`. Shutdown closes every socket with 1001 and frees the port.
- Host A: one Tauri window per placement, labeled `overlay-<n>` (a fresh label each time it is built, because a closed window frees its label only later), bundled `overlay.html?host=native&route=all`, sized to the chosen monitor's full rectangle. Built with `WebviewWindowBuilder` flags `transparent(true)`, `decorations(false)`, `shadow(false)`, `always_on_top(true)`, `skip_taskbar(true)`, `focused(false)`, `focusable(false)` (tao adds `WS_EX_NOACTIVATE`), `resizable(false)`, `content_protected(false)` (capture exclusion goes through `platform::set_capture_excluded`, which refuses below build 19041, never through `content_protected`, which tao applies unconditionally), then `set_ignore_cursor_events(true)` (tao adds `WS_EX_TRANSPARENT | WS_EX_LAYERED`). Tauri does not keep the window out of Alt+Tab and tao rewrites `GWL_EXSTYLE` on every state change, so the platform layer subclasses the HWND (`SetWindowSubclass`) and on `WM_STYLECHANGING` for `GWL_EXSTYLE` forces `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED` on and `WS_EX_APPWINDOW` off. The window is built hidden; on the main thread the guard is installed, click-through applied and the window placed at the chosen monitor's physical rectangle, then it is shown once (tao's `SW_SHOW`, with `WS_EX_NOACTIVATE` already set: tao 0.37.1 clears its "do not focus" marker right after creation, so a later show cannot be `SW_SHOWNOACTIVATE`), placed again and put on top. The platform watcher re-asserts `HWND_TOPMOST` with `SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS` on every foreground window change. The window is never hidden at the OS level after creation: "hiding" Host A (paused rendering, or `host.hide_native_when_obs` while an OBS source is connected) is done by the renderer drawing nothing (the `state`/`config` messages carry what it needs), because a raw `ShowWindow` call desynchronizes tao and a Tauri `show()` may activate. Turning `host.native_overlay` off closes the window; turning it on again, or changing the chosen monitor, its rectangle or its scale, builds a new one. After a DPI change the window puts its physical rectangle back (tao resizes it to keep its logical size), from a short worker thread so the calls run after tao's own `SetWindowPos`. (Phase 1 stack research, `.work/research/stack.md` A.3.)
- Host A listens on its own window (`getCurrentWebviewWindow().listen`), never on the global `Any` target, so the shell's per-window emit filter decides which windows get `keystage://overlay` (Tauri delivers every emit to an `Any` listener whatever the filter says).
- DPI: the app ships its own application manifest (Common-Controls v6 plus `PerMonitorV2`) through `tauri_build::WindowsAttributes::app_manifest`, because tauri-build's default manifest declares no DPI awareness and tao only sets it at runtime.
- Recommended OBS Browser Source sizes, computed by `obsSourceSize(route, config, monitor)` in `src/lib/renderer/sizes.ts` and shown in settings (the app's `src-tauri/src/app/sizes.rs` mirrors it for `server_urls`): `cursor` and `all` are the chosen monitor's pixel size; `keys`, `keyboard` and `mouse` are their content box at the chosen scale (drawing plus the glow bleed on every side plus the offsets), rounded up to even numbers. With the defaults on a 1920 x 1080 monitor: `keys` 884 x 632, `keyboard` 1112 x 352 (full layout with the mouse beside it), `mouse` 168 x 224, `cursor` and `all` 1920 x 1080.
- Inside OBS (`window.obsstudio` exists) a disconnected source is fully transparent and reconnects silently (a status banner must never be recorded). A WebSocket page that is not live (before the first `hello`, or after a disconnect) draws no keyboard, mouse, callouts or cursor effects, in OBS and in a browser. In an ordinary browser, or with `status=1`, a styled "Waiting for IDEA Keystage" card shows after a 600 ms grace period. Host A and the preview never show it. The WebSocket transport reconnects with a backoff from 250 ms doubling to 4 s (plus or minus 20 percent), pings every 10 s and reconnects after 25 s of silence.

## 7. Settings (keystage-core `settings.rs`, stored as JSON in the Tauri app config dir)

```
Settings {
  version: 1, first_run_done: bool,
  host: { native_overlay: bool, hide_native_when_obs: bool, monitor: Option<String> /* device name, e.g. \\.\DISPLAY1 */, exclude_from_capture: bool },
  server: { port: u16 = 47820 },
  overlay: OverlayConfig,
  display: DisplayRules,                                    // general profile defaults; ShortcutsOnly
  profiles: { solidworks_mode: Auto|On|Off, user: Vec<Profile>, solidworks_user_shortcuts: Vec<UserShortcut>,
              solidworks_display: ProfileDisplay /* overrides of the profile's display block; empty by default */ },
  privacy: { excluded_apps: Vec<String> },                  // process names, case-insensitive; password guard is always on and has no switch
  hotkeys: { pause, toggle_keyboard, toggle_solidworks, chapter_marker, session_toggle },   // accelerator strings
  sound: { enabled: false, pack: String, volume: f32 },
  obs: { enabled: false, host: "127.0.0.1", port: 4455, password_dpapi: Option<String> /* base64 DPAPI blob, never plain text */, auto_session: true },
  session: { folder: Option<String>, autosave: true },
  startup: { start_with_windows: false, show_settings: true /* open the settings window at start, except when started by the autostart entry */ },
}
```

Default hotkeys: pause `Ctrl+Alt+Shift+P`, keyboard visual `Ctrl+Alt+Shift+K`, SolidWorks mode `Ctrl+Alt+Shift+S` (steps through Auto, On, Off), chapter marker `Ctrl+Alt+Shift+M`, session log `Ctrl+Alt+Shift+L`. Hotkeys are stored normalized (`Ctrl+Alt+Shift+Win+Key`) and must be registrable by global-hotkey 0.8.0; an unregistrable one falls back to its default with an issue. Default excluded apps: common password managers and Windows credential UI (`1Password.exe`, `Bitwarden.exe`, `KeePass.exe`, `KeePassXC.exe`, `Enpass.exe`, `NordPass.exe`, `Dashlane.exe`, `RoboForm.exe`, `keeperpasswordmanager.exe`, `CredentialUIBroker.exe`, `consent.exe`, `LogonUI.exe`; the list lives in `settings.rs` with a comment that it is a starting list, not sourced data). The keyboard visual switch (`keyboard_visible`) is runtime only and starts on at every launch.

The settings file is `settings.json` in the Tauri app config folder (`%APPDATA%\com.ideabosco.keystage`), written through a temporary file and a rename. An unreadable file is moved to `settings.unreadable-<UTC time>.json`, defaults load, and a notice says so until the next save; a file that lost a section is copied the same way and the lost sections are named.

## 8. Privacy rules (non-negotiable, enforced in code review)

1. The hook callbacks copy a fixed-size POD record (stamped with the password hold word, one atomic load) into a preallocated lock-free queue, unpark the pipeline thread, call `CallNextHookEx` and return. No allocation, no logging, no locks, no syscalls beyond that.
2. Pausing uninstalls both hooks. Resuming reinstalls them and resyncs modifiers. A pause asked before the hooks were handed to the shell is applied the moment they are.
3. A password field (UI Automation `IsPassword`, or an Edit control with `ES_PASSWORD`) or an excluded app in the foreground suppresses everything: no display, no keyboard light, no log entry, no sound. So do a locked session and a foreground the watcher has not identified yet.
   - Between a focus change and its classification, the input receiver holds keys back. It drops them unless the new focus is classified as not a password (see DECISIONS item 6). Keys typed right after a Tab, Enter, Esc, F6 or a mouse click are kept back until the watcher reports where the focus went (at most 150 ms when it reports nothing), and follow that focus's classification. The `Password` event is always sent before the hold changes. A hold that ends without the app suppressing drops modifiers and Caps Lock too.
   - The app shell orders context changes against queued input fail closed: a change that hides applies before the input drained with it; a change that shows more applies only after the input of that pipeline pass and the next one.
   - While UI Automation password detection is not running, typing is masked in every profile (no text runs, no captions for bare typing keys), and `Status.notices` says so.
   - The app's own windows always use the general profile, so typing in the settings window is always masked.
4. Nothing typed is written to disk except a Session Log the user started (by hotkey, tray, settings, or by an OBS recording while `obs.enabled` is on and `obs.auto_session` (default true) is on), and a Session Log stores only the `callout` messages that were displayed plus chapter markers. Masked typing makes no key sound.
5. `tracing` output never contains key names, text, callouts or captions. Lifecycle only. The log is `keystage.log` in the app log folder (`%LOCALAPPDATA%\com.ideabosco.keystage\logs`), 1 MiB per file, 3 files, with a fixed filter (no `RUST_LOG`); settings issues are logged by field name only, and panics log the thread and location only.
6. The server binds `127.0.0.1` only and enforces the Host and Origin rules in section 6.
7. The app makes no outbound network request. The only socket it opens is to the obs-websocket host the user configured (default `127.0.0.1:4455`), and only while OBS is turned on in settings. No telemetry, no update check, no updater plugin.
8. The OBS password is stored only as a DPAPI (CurrentUser) blob. Settings exports leave it out.

## 9. Profiles

```jsonc
{
	"schema": "keystage.profile/1",
	"id": "solidworks",
	"name": "SolidWorks",
	"match": { "process": ["SLDWORKS.exe"] },
	"display": { "mode": "all", "typed_values": true, "mouse_captions": true },
	"visual": { "theme": null },
	"chords": [],
	"sources": [
		{ "id": "...", "url": "...", "title": "...", "version": "2025", "read": "2026-10-01" },
	],
	"shortcuts": [
		{
			"id": "kb-044",
			"keys": ["Ctrl", "B"],
			"doc_keys": "Ctrl + B",
			"command": "Rebuilds the model",
			"description": "...",
			"group": "...",
			"context": "",
			"versions": ["2025", "2026"],
			"source_ids": ["..."],
			"quote": "...",
		},
	],
	"mouse": [
		{
			"id": "ms-003",
			"button": "middle",
			"action": "drag",
			"modifiers": ["Ctrl"],
			"direction": null,
			"command": "Pan",
			"description": "...",
			"context": "",
			"versions": ["2025", "2026"],
			"source_ids": ["..."],
			"quote": "...",
		},
	],
	"differences": [],
	"gaps": [],
}
```

- `data/profiles/solidworks.json` is sourced data only. Nothing in it is written from memory. Code never adds SolidWorks captions of its own. The file also carries `data_note` and `read` at the top level, and an entry or source may carry `"evidence": "screenshot"` (with `image_url` and `image_sha256` on the source) when its label was read from a Dassault screenshot rather than page text. The profile model keeps unknown fields, so the file round-trips exactly.
- Mouse entries with an action outside click, double_click, drag, wheel and gesture are kept but never matched (the data has one, `ms-020`, action `press`). Button `unspecified` ("drag" or "click" with no button named in the documentation) is matched to the left button for click, double click and drag; that is an interpretation, documented at `CaptionIndex::lookup_mouse`. Drag and click entries that carry a direction are never matched, because drags are not classified by direction.
- The core embeds the built-in profiles with `include_str!`. User profiles live in settings. The general profile (no match, ShortcutsOnly, no captions) is the fallback.
- Profile switching: on each foreground change the app finds the first profile whose `match.process` contains the foreground exe name (case-insensitive). `solidworks_mode`: `auto` follows the foreground, `on` forces the SolidWorks profile, `off` never uses it.
- User shortcut import (keystage-core `import.rs`), both routes unvalidated until a real export is supplied (DECISIONS item 4):
  - **Primary: the Keyboard tab's Copy List** (SolidWorks Tools > Customize > Keyboard, Category "All Commands", Copy List), pasted into the import box or loaded from a `.txt`. Parsed defensively: delimiter and header detection, UTF-8 with or without BOM, UTF-16LE with BOM, columns Category, Command, Shortcut(s), several shortcuts per cell, unknown keys kept as warnings, duplicate bindings warned.
  - **Experimental: a `.sldreg` file** from the SOLIDWORKS Settings Wizard. The format is undocumented, so the importer sniffs the bytes (empty, binary, Registration Entries text in UTF-8 or UTF-16LE) and, lacking a documented keyboard section, returns a clear "could not read the keyboard section; use Copy List" result instead of guessing.
  - Imports produce `UserShortcut { keys, doc_keys, command, category }` entries, shown as a preview of added, changed and unchanged captions against the sourced defaults, and stored in settings only after the user applies them. They carry `source: "user-import"` so captions from the user's own SolidWorks are never confused with the sourced data file.
- Caption choice when several entries match the same input (for example `Space` has entries for several contexts): an entry with an empty `context` wins; otherwise the first entry in file order; the callout carries that entry's `context` so the renderer can show it in small type. User-imported shortcuts win over the data file for the same keys.

## 10. Session Log and exports (crate `keystage-session`)

- A session holds `callout` snapshots (final state of each callout id), chapter markers `{ t_ms, name }`, start time, and an optional OBS timecode mapping (record duration at a known monotonic instant, paused intervals).
- Exports: `.srt` (one cue per callout: `Ctrl + B  Rebuilds the model`), Markdown ("Shortcuts used in this video", unique shortcuts with command names and counts, ASCII `x4`), JSON (`keystage.session/1`, documented in `docs/SESSION_FORMAT.md`), YouTube chapter lines (`00:00 Start`, then markers; YouTube needs the first at 00:00). Each export returns its text plus warnings (`ExportWarning::message()`); the crate writes no file except `Session::save`.
- Sessions are saved as `<id>.json` in `settings.session.folder` or `Documents\IDEA Keystage\Sessions`, only when a session stops (with `autosave` on). A running session is stopped and saved on Quit.

## 11. Look

- Tokens and the plate recipe are ported (copied, not imported) from `pina-hash/idea-app` into `src/lib/styles/tokens.css` and `src/lib/styles/plate.css`. Raised means pressable, inset means status. One geometry, per-theme color blocks.
- Fonts: Rajdhani, Share Tech Mono, Orbitron from `@fontsource`, bundled. No other fonts, no CDN.
- No grid backgrounds anywhere. Color is never the only signal (a pressed key also changes shape). 44 px minimum targets in the settings window. Contrast is measured.
- Browser floor: OBS 30 embeds Chromium 103, OBS 31 and 32 embed Chromium 127 (DECISIONS item 5). Vite builds to `chrome103` (JS and CSS). Overlay code must not depend on anything newer than Chromium 103 without a fallback that looks finished on its own: no `:has()`, `@container`, `color-mix()`, `oklch()`, CSS nesting, `popover`, `@starting-style`, `translate` property, dynamic viewport units, `Array.prototype.toSorted`, `Promise.withResolvers`. `corner-shape` is progressive enhancement only.
- OBS injects its Custom CSS (`body { background-color: rgba(0, 0, 0, 0); margin: 0px auto; overflow: hidden; }` by default) after load, so the overlay sets its own transparent, margin-free, full-size root and never relies on body margins.
- OBS reloads a source on scene activation when "Refresh browser when scene becomes active" is on and destroys it when "Shutdown source when not visible" is on, so the overlay reconnects fast and the server sends a full hello snapshot on every connect.
- Everything is vector or CSS (SVG, CSS shapes, canvas drawn at devicePixelRatio). No raster art in the overlay.

## 12. Settings window IPC (Tauri commands)

`src/settings/ipc.ts` is the typed client; it falls back to an in-memory mock backend when `window.__TAURI_INTERNALS__` is absent (browser previews, screenshots, unit tests). `src-tauri/src/app/commands.rs` implements exactly these names and argument shapes (Tauri converts camelCase JS argument keys to snake_case Rust parameters).

| Command                          | Args                                                           | Returns                                                                                                                                                                  |
| -------------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `get_settings`                   |                                                                | `Settings`                                                                                                                                                               |
| `save_settings`                  | `{ settings }`                                                 | `Settings` (validated, persisted, pushed to every host as `config`)                                                                                                      |
| `reset_settings`                 |                                                                | `Settings`                                                                                                                                                               |
| `get_status`                     |                                                                | `Status` (below, including `notices`)                                                                                                                                    |
| `list_monitors`                  |                                                                | `Monitor[]` `{ id, name, x, y, width, height, scale, primary }` (physical pixels)                                                                                        |
| `set_paused`                     | `{ paused }`                                                   | `Status`                                                                                                                                                                 |
| `set_solidworks_mode`            | `{ mode: "auto" \| "on" \| "off" }`                            | `Status`                                                                                                                                                                 |
| `set_keyboard_visible`           | `{ visible }`                                                  | `Status`                                                                                                                                                                 |
| `obs_test`                       | `{ host, port, password: string \| null }`                     | `{ ok, obs_version?, ws_version?, error? }`                                                                                                                              |
| `obs_set_password`               | `{ password: string \| null }`                                 | `{ stored: boolean }` (DPAPI; null clears)                                                                                                                               |
| `shortcuts_import_preview`       | `{ source: { kind: "text", text } \| { kind: "file", path } }` | `{ format: "copy_list" \| "sldreg" \| "unknown", entries: UserShortcut[], added: Change[], changed: Change[], unchanged: number, warnings: string[], validated: false }` |
| `shortcuts_import_apply`         | same `source`                                                  | `Settings`                                                                                                                                                               |
| `shortcuts_import_clear`         |                                                                | `Settings`                                                                                                                                                               |
| `profile_shortcuts`              | `{ id }`                                                       | the merged profile (built-in data plus user overrides) for display                                                                                                       |
| `session_start` / `session_stop` |                                                                | `Status`                                                                                                                                                                 |
| `session_marker`                 | `{ name: string \| null }`                                     | `Status`                                                                                                                                                                 |
| `sessions_list`                  |                                                                | `SessionSummary[]` `{ id, started, duration_ms, callouts, markers }`                                                                                                     |
| `session_get`                    | `{ id }`                                                       | the session JSON (`keystage.session/1`)                                                                                                                                  |
| `session_rename_marker`          | `{ id, index, name }`                                          | session JSON                                                                                                                                                             |
| `session_export`                 | `{ id, format: "srt" \| "md" \| "json" \| "chapters", path }`  | `{ path, warnings: string[] }`                                                                                                                                           |
| `session_delete`                 | `{ id }`                                                       | `SessionSummary[]`                                                                                                                                                       |
| `settings_export`                | `{ path }` / `settings_import` `{ path }`                      | `Settings`                                                                                                                                                               |
| `server_urls`                    |                                                                | `{ port, running, error, routes: { route, url, width, height }[] }`                                                                                                      |
| `play_sound_preview`             | `{ pack }`                                                     | `null`                                                                                                                                                                   |
| `open_external_folder`           | `{ which: "sessions" \| "config" }`                            | `null`                                                                                                                                                                   |
| `overlay_hello`                  |                                                                | the `hello` message (Host A and the preview call it on start)                                                                                                            |
| `hotkeys_capture`                | `{ active }`                                                   | `null` (unregisters the hotkeys while a hotkey field captures; they return on `false` or after 30 s)                                                                     |
| `hotkeys_status`                 |                                                                | `[{ name, accelerator, registered, error }]`                                                                                                                             |
| `platform_info`                  |                                                                | `{ os_build: number \| null, exclude_from_capture_supported: boolean }`                                                                                                  |

Every command has its own Tauri permission (`tauri_build::AppManifest::commands`, generated into `src-tauri/permissions/autogenerated/`). The `settings` window gets the app commands, event listen and unlisten, dialog open and save, clipboard write and `opener:allow-open-url` for `https://*` only; `overlay-*` windows get event listen and unlisten and `allow-overlay-hello` only. No window has file system or shell permissions. Adding a command needs the name in `build.rs`, `commands.rs` and `capabilities/settings.json`; a unit test checks all three against `src/settings/ipc.ts`.

```ts
type Status = {
	paused: boolean;
	suppressed: null | 'password' | 'excluded';
	profile: { id: string; name: string } | null;
	solidworks_mode: 'auto' | 'on' | 'off';
	keyboard_visible: boolean;
	server: { port: number; running: boolean; error: string | null; clients: number };
	obs: {
		enabled: boolean;
		state: 'off' | 'connecting' | 'connected' | 'auth_failed' | 'error';
		error: string | null;
		recording: null | { active: boolean; paused: boolean; timecode: string };
	};
	session: null | {
		id: string;
		active: boolean;
		started: string;
		callouts: number;
		markers: number;
	};
	hook: { installed: boolean; dropped: number }; // dropped = queue overflow count, should stay 0
	version: string;
	notices: string[]; // plain sentences the user should read: input capture or app detection did not start, password detection
	// unavailable, a session log not saved, a hotkey another program holds, OBS, sound, start with Windows,
	// a settings file reset, the pipeline stopped after an error
};
```

Events the app emits to the settings window: `keystage://status` (Status, on change, checked at least once a second), `keystage://settings` (after every stored settings change, from the tray, a hotkey, an import, a reset or a save; payload the settings without `obs.password_dpapi`, which the window may ignore and read `get_settings` instead), `keystage://overlay` (the same protocol messages Host A receives, for the live preview, serialized once). The settings window saves by reading `get_settings` and sending it with only the values it edited (a three-way merge), so a change made from the tray or a hotkey is never overwritten. Dialogs (open `.sldreg`, save exports) use `@tauri-apps/plugin-dialog` from the front end, which then passes the chosen path to the command.

The app CSP (`tauri.conf.json`) is `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; media-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost ws://127.0.0.1:*; object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'` with `dangerousDisableAssetCspModification: ["style-src"]`: Tauri otherwise adds a nonce to `style-src`, and with a nonce present browsers ignore `'unsafe-inline'`, which would block the static inline `style` attributes in the Svelte templates. `script-src` keeps Tauri's hashes and nonces.
