# Decisions

Phase 1 audit and research for IDEA Keystage v1, answered before any application code was written, then the decisions made while building. Every answer names its source (URL and date read) or the command that was run. Detailed research notes behind each answer were kept in the session's working directory (`.work/research/`, not committed); the facts that matter are copied here.

Session start: `git fetch origin main` then `git rev-parse HEAD origin/main` gave `9bee0b172d8a64d076e0160b34b4388c20ca9f56` for both (2026-10-01).

## Phase 1

### 1. What is in the repository

Command: `git ls-tree -r HEAD` at `9bee0b1` (2026-10-01).

```
100644 blob 877b1296a25bec540930fb2c036356b00fb66c63	README.md
```

`README.md` contained the single line `# idea-keystage`. One commit (`9bee0b1 Initial commit`). The repository held only a README, as expected, so the audit continued.

### 2. The stack

**Decision: Tauri 2 (Rust core) with a Svelte 5 and TypeScript front end, as proposed.** The overlay window requirements are achievable on Windows 10 and 11 with Tauri 2.12.1, with three pieces of raw Win32 the research identified.

Sources (all read 2026-10-01): https://v2.tauri.app/learn/window-customization/, https://v2.tauri.app/reference/config/, https://v2.tauri.app/reference/javascript/api/namespacewindow/, https://docs.rs/tauri/2.12.1/x86_64-pc-windows-msvc/tauri/webview/struct.WebviewWindow.html, https://v2.tauri.app/distribute/windows-installer/, https://v2.tauri.app/plugin/updater/, https://v2.tauri.app/security/csp/, https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles, https://learn.microsoft.com/en-us/windows/win32/shell/taskbar, https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process; plus the source of tauri 2.12.1, tao 0.37.1 and wry 0.57.0 from their crates.io tarballs (checksums matched against the index).

| Requirement | How | Source |
|---|---|---|
| Transparent | `transparent(true)`; wry sets the WebView2 background to (0,0,0,0); page sets `background: transparent` | tao `window.rs`, wry `webview2/mod.rs` |
| No border or rounded shadow | `shadow(false)` (the default `true` gives an undecorated window a 1px border and Windows 11 rounded corners) | Tauri config reference |
| Always on top | `always_on_top(true)` (`WS_EX_TOPMOST`, `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE)`), re-asserted on foreground change | tao `window_state.rs` |
| Never takes focus | `focused(false)` (first show is `SW_SHOWNOACTIVATE`) and `focusable(false)` (tao adds `WS_EX_NOACTIVATE`) | tao `window_state.rs` |
| Click-through | `set_ignore_cursor_events(true)` (tao adds `WS_EX_TRANSPARENT \| WS_EX_LAYERED`) | tao `window_state.rs` |
| Not in the taskbar | `skip_taskbar(true)` (`ITaskbarList::DeleteTab`) | tao `window.rs` |
| Not in Alt+Tab | **raw Win32**: `WS_EX_TOOLWINDOW` on, `WS_EX_APPWINDOW` off, held in place by a `SetWindowSubclass` handler on `WM_STYLECHANGING`, because tao rewrites `GWL_EXSTYLE` from its own flags on every state change | Microsoft extended window styles and taskbar pages; tao `window_state.rs` lines 418 to 431 |
| Per-monitor DPI v2 | **own application manifest** through `tauri_build::WindowsAttributes::app_manifest` (`src-tauri/app.manifest`), because tauri-build's default manifest declares only Common Controls v6 and tao sets PMv2 at runtime, which Microsoft advises against | tauri-build 2.7.1 `src/windows-app-manifest.xml`; tao `dpi.rs`; Microsoft DPI page |
| HWND access | `WebviewWindow::hwnd()` returns `windows` 0.62 `HWND`, so the app pins `windows = 0.62.2` | docs.rs Windows target page |

What changed in the design because of this answer:

* The prompt assumed `WS_EX_NOACTIVATE` needs raw Win32. It does not: `focusable(false)` sets it. Raw Win32 is needed for Alt+Tab (`WS_EX_TOOLWINDOW`), and the styles need a `WM_STYLECHANGING` guard to survive tao.
* Host A is never hidden at the OS level after it is created. A raw `ShowWindow` call desynchronizes tao's visibility flag, and a later Tauri `show()` may activate the window. Hiding Host A (OBS sources in use, paused) is done by the renderer drawing nothing; turning it off for good closes the window.
* Host A and the settings preview receive events through Tauri events, not the WebSocket. Chromium's Local Network Access rules reached WebSockets in Edge 147 and WebView2 plans to make them API-controlled (https://github.com/MicrosoftEdge/WebView2Announcements/issues/126, read 2026-10-01), so keeping Tauri pages off the socket removes that risk. The server's Origin allowlist therefore stays limited to `http://127.0.0.1:<port>` and `http://localhost:<port>`.
* Prior art: Keyviz (https://raw.githubusercontent.com/mulaRahul/keyviz/main/src-tauri/tauri.conf.json, read 2026-10-01) is a Tauri 2 keystroke visualizer using the same window flags, which supports the choice. Its code was not used.

Pinned versions (read 2026-10-01 from https://index.crates.io and https://registry.npmjs.org; Cargo pins are exact `=x.y.z`, both lockfiles are committed):

| Rust crate | Version | | npm package | Version |
|---|---|---|---|---|
| tauri | 2.12.1 | | @tauri-apps/cli | 2.12.1 |
| tauri-build | 2.7.1 | | @tauri-apps/api | 2.12.1 |
| tauri-plugin-single-instance | 2.5.2 | | @tauri-apps/plugin-global-shortcut | 2.4.0 |
| tauri-plugin-global-shortcut | 2.4.0 | | @tauri-apps/plugin-autostart | 2.7.0 |
| tauri-plugin-autostart | 2.7.0 | | @tauri-apps/plugin-dialog | 2.8.1 |
| tauri-plugin-dialog | 2.8.1 | | @tauri-apps/plugin-clipboard-manager | 2.4.1 |
| tauri-plugin-clipboard-manager | 2.4.1 | | @tauri-apps/plugin-opener | 2.7.0 |
| tauri-plugin-opener | 2.7.0 | | svelte | 5.57.1 |
| windows | 0.62.2 | | @sveltejs/vite-plugin-svelte | 7.3.1 |
| axum | 0.8.9 | | vite | 8.3.1 |
| tokio | 1.53.1 | | svelte-check | 4.7.6 |
| tokio-tungstenite | 0.29.0 (what axum 0.8.9 `ws` requires) | | typescript | 6.0.3 (7.x is outside svelte-check's peer range) |
| futures-util | 0.3.34 | | vitest | 5.0.3 |
| serde / serde_json | 1.0.229 / 1.0.151 | | jsdom | 29.1.1 (30.x needs Node 22.22.2; this container has 22.22.0) |
| crossbeam-channel / crossbeam-queue | 0.5.17 / 0.3.14 | | prettier / prettier-plugin-svelte | 3.9.9 / 4.1.1 |
| parking_lot | 0.12.5 | | @types/node | 26.6.3 |
| mime_guess | 2.0.5 | | @fontsource/rajdhani, share-tech-mono, orbitron | 5.3.0 each |
| rodio | 0.22.2 (`playback`, `wav` only) | | playwright (scripts only) | 1.56.1, matching the installed Chromium 1194 |
| base64 | 0.22.1 | | | |
| sha2 | 0.11.0 | | | |
| thiserror / anyhow | 2.0.21 / 1.0.104 | | | |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | | | |
| tempfile (tests) | 3.27.0 | | | |

Toolchain: `rustc 1.97.0`, `cargo 1.97.0` (tauri 2.12.1 declares `rust-version = 1.90`), Node `v22.22.0`, npm `10.9.4`.

### 3. SolidWorks shortcuts and mouse controls

**Answer: the SolidWorks profile data is sourced only from Dassault's SOLIDWORKS Design Help on help.solidworks.com for 2025 and 2026, and lives in `data/profiles/solidworks.json`, where every entry carries its URL, version and date read (2026-10-01) plus a verbatim quote.** Two extraction agents read the help topics (keyboard and view orientation; mouse, middle button, wheel and gestures), then two independent verifiers re-fetched every cited page and removed or corrected anything the quote did not support. Final counts, the 2025 versus 2026 differences and the full gap list are below.

How the pages were read: help.solidworks.com is a Next.js site that answers plain `curl` with HTTP 403 (Akamai) but serves a topic, with a browser user agent, whose body sits in the page's `__NEXT_DATA__` JSON at `props.pageProps.helpContentData.helpText`. A helper script extracted that text for each topic URL, `https://help.solidworks.com/<version>/english/SolidWorks/<file>`. Starting topics: `sldworks/r_keyboard_shortcuts.htm` (Keyboard Shortcuts), `c_time_saving_keyboard_shortcuts.htm` (Selected Keyboard Shortcuts), `HIDD_CUSTOMIZE_KEYBOARD.htm`, `r_Middle_Mouse_Button.htm` (Middle Mouse Button Functions) and `c_mouse_sestures.htm` (Mouse Gestures; Dassault's filename is misspelled), then every linked topic.

**Counts (after verification and the second search).** 367 keyboard entries and 183 mouse entries, citing 294 source pages (139 for 2025, 155 for 2026). 528 entries appear in both versions, 12 only in 2026, 10 only in 2025 (8 of them the screenshot-sourced standard views below). The keyboard verifier checked 361 extracted entries, moved 2 whose quotes named no mouse action into the shortcut list, corrected 69 groups of fields (mostly quotes that did not contain the key or every word of the caption), and added 32 entries the extractor missed. The mouse verifier checked 191 entries, removed none, corrected 25 and added 20. Merging removed 36 duplicate shortcuts and 26 duplicate mouse entries that both agents had found.

**2025 versus 2026.** No documented mouse binding, mouse gesture rule or view option changed. The keyboard differences are small and are all recorded in the data file's `differences` list (31 items). The ones a user could notice:
* Selection filters: 2025 lists E (edges), X (faces) and V (vertices); 2026 adds F6 (Toggle Selection Filters) and notes more filters that can be assigned keys (`r_selection_filter.htm`).
* Autotransitioning: in both versions A switches between line and arc; 2026 adds that it is "Available when Enable on screen numeric input on entity creation is cleared" (`t_Autotransitioning.htm`).
* Search: 2026 adds "To find and run commands, click Search Commands or press S." (`c_search.htm`); W and S for search are in both.
* Relation groups (Ctrl + select) exist only in 2026; several 3DEXPERIENCE-only Alt + drag statements appear only in 2026.
* 2026 renames the product "SOLIDWORKS Design" throughout; the mouse gesture tutorial with named 8-gesture mappings exists only on the 2026 site.

**Gaps (no Dassault page for 2025 or 2026 documents these, so the data has no entry for them).** These are reported, not filled from memory:
1. **Ctrl + number standard view shortcuts** (Front, Isometric, Normal To and so on) are not documented in **text** for either version; see the second search below, which found them in a 2025 screenshot. Searched: Standard Views Toolbar, Orientation, Orientation Dialog Box, View Selector, View Toolbar, Heads-up View Toolbar, Selected Keyboard Shortcuts, and the site search for "Ctrl+8", "Ctrl+1", "Isometric Ctrl" in both versions.
2. **A complete list of default keyboard shortcuts.** It is not published as a help topic; Dassault directs users to the Keyboard tab's Print List or Copy List. This is why the Copy List import (item 4) matters: it brings the user's own full list, including standard view keys, into the captions.
3. Middle mouse button double-click (for example zoom to fit): not documented.
4. Which wheel direction zooms in by default: not documented (only the "Reverse mouse wheel zoom direction" option); wheel entries match both directions.
5. Factory default mouse gesture assignments per document type: shown only as unlabeled images; the 2026 online tutorial names a few 8-gesture mappings, which are in the data as 2026 only.
6. The 2025 online tutorials ("swtutorialonline") do not exist on the 2025 site.
7. Gesture directions for 2, 3, 4 and 12 gesture guides are not documented in text.
8. Access keys other than the Alt+F example are not enumerated; walk-through speed number keys are not named; "Alt + specified values" symbol entry and generic "hold Ctrl and select" mentions across feature topics were not encoded one by one.
9. View topics with no keyboard shortcut at all (Previous View, Zoom to Area, Zoom to Selection, Zoom to Sheet, Viewports, Model View, Predefined Views).
10. Guides outside the core `sldworks` help (Simulation, Motion, Routing, Toolbox) were not mined.

The full list of 17 gaps, each with the URL checked and the reason, is the `gaps` array in `data/profiles/solidworks.json`.

**Second search for gaps 1, 3, 4 and 5** (an agent pattern-searched every topic in the help table of contents for both versions, 6,097 for 2025 and 6,251 for 2026, plus the full What's New sets, the 2026 tutorials, the API help, and 288 site searches; 15,452 page texts in all, 2026-10-01):
* Gap 1 is **closed for 2025 only, by a screenshot.** Dassault's What's New topic "Search Commands (2025 SP2)" (https://help.solidworks.com/2025/english/WhatsNew/c_ui_search_commands.htm) says "When you use Search Commands, the results include the tools' keyboard shortcuts in parenthesis if they exist." and shows a screenshot (https://help.solidworks.com/2025/english/WhatsNew/krt1738080155112.image, SHA-256 `150a8aed109ea30d5984964e9f2b10aef80bbc254fa5ce97df94556a5b9835c8`) whose labels read "Front (Ctrl+1)", "Back (Ctrl+2)", "Left (Ctrl+3)", "Right (Ctrl+4)", "Top (Ctrl+5)", "Bottom (Ctrl+6)", "Isometric (Ctrl+7)", "Normal To (Ctrl+8)". Two independent readers (the agent, then the orchestrator viewing the downloaded PNG) read the same labels, and the same screenshot's "View Orientation (SpaceBar)" and "View Selector (Ctrl+SpaceBar)" agree with shortcuts the help states in text. **Decision: these eight are in the data as version 2025 entries marked `"evidence": "screenshot"`**, with the image URL and hash in their source. This is image evidence, not page text; Mr. Pina can remove them by deleting the eight `kb-360` to `kb-367` entries. No 2026 page documents them in text or image, so for 2026 they remain a gap; the app shows the caption whenever the SolidWorks profile is active, because it cannot tell 2025 from 2026.
* Gaps 3 (middle double-click), 4 (wheel direction) and 5 (default gestures) remain. The only matches were for separate products (SOLIDWORKS Treehouse for a middle double-click "zoom extents", eDrawings for the wheel direction), which do not apply to SOLIDWORKS document windows.

What changed in the design because of this answer:
* Captions are shown only for sourced entries. Because many sourced entries are context-specific (the same key does different things in a sketch, an assembly or a dialog), each caption carries its context in small type, and an entry with no stated context wins a tie.
* "drag" or "click" with no button named in the documentation is kept as button `unspecified` in the data. The code matches it to a left-button drag or click. That is an interpretation, not a documented fact (the pages say "drag" without naming a button), and the code says so where it does it.
* The Copy List import became the way to get complete captions, since the documentation itself is incomplete.

### 4. Exporting and reading customized keyboard shortcuts

**Decision: the primary import is the Keyboard tab's Copy List (pasted text or a saved `.txt`). The `.sldreg` import is experimental. Both are unvalidated until Mr. Pina supplies a real export from SolidWorks 2025 and 2026.**

What Dassault documents (all read 2026-10-01, identical in substance for 2025 and 2026 unless noted):

* The tool is the **SOLIDWORKS Settings Wizard** (Tools > Save/Restore Settings). "Keyboard shortcuts" is one of six categories it can save: "You can save or restore system settings for: System options Toolbar layout (All toolbars or Macro toolbar only) Keyboard shortcuts Mouse gestures Menu customization Saved views" (https://help.solidworks.com/2025/english/SolidWorks/sldworks/c_solidworks_settings_wizard.htm and the 2026 page). "The *.sldreg file is generated by the SOLIDWORKS Settings Wizard" (https://help.solidworks.com/2025/english/Installation/install_guide/t_settings_options_welcome.htm and 2026). The API equivalent is `ISldWorks::SaveSettings`, "Saves the specified SOLIDWORKS settings to the specified *.sldreg file" (https://help.solidworks.com/2025/english/api/sldworksapi/SolidWorks.Interop.sldworks~SolidWorks.Interop.sldworks.ISldWorks~SaveSettings.html and 2026).
* The settings are saved "to a file" and restored "to the following registries: ... CURRENT_USER of current user ... LOCAL_MACHINE of selected computers" (Settings Wizard page). **No 2025 or 2026 page states the `.sldreg` file format, its encoding, whether it is a `.reg` text export, or any registry key path for keyboard shortcuts.** The research searched the Design Help, the whole Installation and Administration guide (157 topics per version, walked in table-of-contents order), the API help and the 2026 tutorials.
* The Keyboard tab of Tools > Customize documents two more exports: "Copy List: Click to copy the selected list to the clipboard so you can paste it into documents such as Word or Excel" and "Print List: ... print the selected list with columns for Category, Command, and Shortcut(s)"; also "Letters are shown in uppercase" and that assigning a shortcut already in use removes it from the old command (https://help.solidworks.com/2025/english/SolidWorks/sldworks/HIDD_CUSTOMIZE_KEYBOARD.htm and 2026). "Print or copy a list of shortcut keys from the Keyboard tab of the Customize dialog box" (https://help.solidworks.com/2025/english/SolidWorks/sldworks/r_keyboard_shortcuts.htm and 2026).

Claims in the prompt that were wrong:

* "Copy Settings Wizard": that was the name through the 2022 help; the 2023 help renamed it the SOLIDWORKS Settings Wizard (https://help.solidworks.com/2022/english/SolidWorks/sldworks/c_copy_settings_wizard.htm versus https://help.solidworks.com/2023/english/SolidWorks/sldworks/c_solidworks_settings_wizard.htm).
* ".sldreg is a registry export": not stated by any 2025 or 2026 page. It may be one; it is not documented.
* ".sldreg is the expected route": the Copy List is documented better (its purpose and its three columns), so it is the primary route.

What was built from this:

* `Copy List` parser: tab, comma or semicolon delimiter detection; optional header row; UTF-8 with or without BOM and UTF-16LE with BOM; CRLF or LF; several shortcuts in one cell; lowercase letters normalized; unknown keys and malformed rows kept as warnings; duplicate bindings warned. Tested against fixtures constructed from the documented columns, with synthetic command names (`Fixture Command One`), never real SolidWorks names.
* `.sldreg` sniffer: empty, binary, and Registration Entries text (UTF-8 or UTF-16LE) are recognized and tokenized, but with no documented keyboard section it reports "could not read the keyboard section; use Copy List" rather than guess. Its fixtures are labeled hypothesis fixtures.
* Both routes show a preview of added and changed captions before anything is applied, and imported entries are tagged as coming from the user's own SolidWorks.

Validation still required (Mr. Pina, once): with SolidWorks 2025 and 2026, save a `.sldreg` with only "Keyboard shortcuts" selected, and paste a Copy List of "All Commands" into Notepad. With those two files the parsers can be validated and the hypothesis fixtures replaced.

### 5. OBS integration surfaces

**Browser Source** (https://obsproject.com/kb/browser-source, read 2026-10-01, page dated 2022-01-11; behavior details from the obs-browser source at commit `3f0a2cdf378939ebe3c6f9ab36d4ea100c25aac2`, the one pinned by OBS 32.2.2, https://raw.githubusercontent.com/obsproject/obs-browser/3f0a2cdf378939ebe3c6f9ab36d4ea100c25aac2/obs-browser-plugin.cpp, read 2026-10-01):

* Width and Height default to 800 and 600 (range 1 to 8192). The default Custom CSS is `body { background-color: rgba(0, 0, 0, 0); margin: 0px auto; overflow: hidden; }`, injected as a `<style>` after the page loads, so it overrides page rules of equal specificity. "Shutdown source when not visible" unloads the page; "Refresh browser when scene becomes active" reloads it when the source becomes active; both default off. "Use custom frame rate" defaults off on Windows (the page renders at the canvas frame rate).
* Inside OBS, `window.obsstudio` exists (obs-browser README), which is how the overlay knows it is in OBS.
* Loading `http://127.0.0.1` or `http://localhost` is **not explicitly documented**. The obs-browser source has no host filter for the URL field, and a 2018 forum thread reports loopback working (community evidence only). This must be confirmed on Mr. Pina's OBS.
* A "Local file" page gets the origin `http://absolute`, so Keystage sources use URL mode with `http://127.0.0.1:<port>/overlay/<route>`.
* Embedded Chromium: OBS 30.x uses CEF branch 5060 (Chromium **103**), OBS 31.x and 32.x use branch 6533 (Chromium **127**). OBS documents only the 31.0.0 change ("Updated the browser source/docks CEF (Chromium) to version 127 (6533)", release notes); the others come from `buildspec.json` / `CMakePresets.json` at each tag on raw.githubusercontent.com mapped through https://chromiumdash.appspot.com/fetch_milestones (read 2026-10-01).

**obs-websocket 5** (https://raw.githubusercontent.com/obsproject/obs-websocket/master/docs/generated/protocol.md and the README, read 2026-10-01; byte-identical to the copy pinned by OBS 32.2.2):

* Built into OBS 28 and later. Default port 4455 (README). The server is **disabled by default** and authentication is on by default with a generated 16-character password (`src/Config.h`, `Config.cpp`). The menu item is Tools > WebSocket Server Settings ("Enable WebSocket server", "Show Connect Info").
* Connection: server `Hello` (op 0, with `authentication.challenge` and `salt`), client `Identify` (op 1, `rpcVersion: 1`, `authentication`, `eventSubscriptions`), server `Identified` (op 2), then `Request` (op 6) / `RequestResponse` (op 7) and `Event` (op 5).
* Authentication, verbatim from the protocol: concatenate password and salt, SHA-256, base64 (the "base64 secret"); concatenate that with the challenge, SHA-256, base64. The challenge is per connection, so it is recomputed every time. The protocol's own Identify example does not match its Hello example; the client is tested with a vector computed from the documented algorithm.
* Subscriptions: `eventSubscriptions: 65` (General 1 + Outputs 64). `RecordStateChanged` is in Outputs; its `outputState` is one of `OBS_WEBSOCKET_OUTPUT_STARTING|STARTED|STOPPING|STOPPED|PAUSED|RESUMED` for recording, and `outputActive` is false while paused, so state is derived from `outputState`.
* `GetRecordStatus` returns `outputActive`, `outputPaused`, `outputTimecode` (`HH:MM:SS.mmm`), `outputDuration` (milliseconds) and `outputBytes`. Keystage seeds state with it after `Identified` and polls it each second while recording to map its own clock to recording time.
* `CreateRecordChapter` exists from obs-websocket 5.5.0 (OBS 30.2) and works only with Hybrid MP4 (and Hybrid MOV from OBS 32.0). It fails with code 702 for MKV, which is the default container in OBS 30.2 and 31 release builds. Keystage therefore always records chapter markers in its own Session Log and mirrors them into OBS only when supported.
* Close code 4009 means authentication failed (stop retrying until the password changes); 4011 means kicked (do not reconnect automatically).

What changed in the design because of this answer:

* The overlay is built to a **Chromium 103** floor (`build.target` and `build.cssTarget` `chrome103`) so the same sources work in OBS 30, 31 and 32. Overlay CSS avoids `:has()`, `@container`, `color-mix()`, `oklch()`, nesting and other features newer than 103 unless a finished-looking fallback exists.
* The overlay sets its own transparent, margin-free, full-size root, because OBS's Custom CSS is user-editable and centers the body with `margin: 0px auto`.
* The server sends a full snapshot on every connect, since OBS sources reload on scene activation when that option is on.
* The settings window tells the user to enable the WebSocket server in OBS first, and suggests Hybrid MP4 when chapter markers should also land in the recording file.

Claims in the prompt that were wrong or imprecise: the OBS knowledge base still says "obs-websocket Settings" where the current menu says "WebSocket Server Settings"; "Control audio via OBS" is not in the KB's property table (it is in the source); the protocol document does not state the default port (the README does).

### 6. Password fields and the foreground process

**Password fields: two checks, fail closed.**

* UI Automation: `UIA_IsPasswordPropertyId` (30019) "indicates whether the automation element contains protected content or a password. When the IsPassword property is TRUE and the element has the keyboard focus, a client application should disable keyboard echoing or keyboard input feedback" (https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-automation-element-propids, read 2026-10-01). Edit controls "Must be set to TRUE on edit controls that contain passwords" (https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supporteditcontroltype), so detection is only as good as the target app's accessibility support.
* Threading: UIA calls belong on "a separate thread. ... This thread should not own any windows, and should be a ... Multithreaded Apartment (MTA) model thread" (https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading). Property reads are cross-process ("slow and inefficient because it requires a cross-process call for each property", https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients), so the design subscribes once with `AddFocusChangedEventHandler` and a cache request for IsPassword, on its own MTA thread, and reads the cached value in the handler. Implementing the handler needs `windows-core` as a direct dependency (`#[implement]`).
* Cheap Win32 check for classic controls: `GetGUIThreadInfo` (its `hwndFocus`, https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getguithreadinfo) plus class `Edit` and the `ES_PASSWORD` style (https://learn.microsoft.com/en-us/windows/win32/controls/edit-control-styles). It sends no message to the target, so it cannot hang, but browser, Electron, WPF and UWP password boxes are not separate HWNDs and need UIA.
* Fail closed: while either check says password, nothing is shown, logged or sounded; if a focus change cannot be classified in time, the previous suppression state is kept rather than showing keys.

**Foreground process: on every foreground change, cheaply.**

* `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, ..., WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)`; "The client thread that calls SetWinEventHook must have a message loop in order to receive events" and out-of-context events are "delivered on the same thread that called SetWinEventHook" (https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook, read 2026-10-01). It runs on a dedicated watcher thread, never the hook thread.
* Name lookup: `GetWindowThreadProcessId`, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`, `QueryFullProcessImageNameW`, `CloseHandle`, cached per process id (https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew, https://learn.microsoft.com/en-us/windows/win32/procthread/process-security-and-access-rights). A NULL foreground window or an `OpenProcess` failure is "unknown", never "not excluded".
* No Microsoft page states whether a medium-integrity process can query an elevated one. While an elevated window is foreground the hooks most likely see no input at all (UIPI, https://learn.microsoft.com/en-us/previous-versions/dotnet/articles/bb625963(v=msdn.10)), so the overlay shows nothing stale and resyncs when focus returns.

### 7. Low-level hook constraints

Both claims are confirmed by Microsoft (https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc and https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc, read 2026-10-01):

* Message loop: "This hook is called in the context of the thread that installed it. The call is made by sending a message to the thread that installed the hook. Therefore, the thread that installed the hook must have a message loop."
* Timeout: "The hook procedure should process a message in less time than the data entry specified in the LowLevelHooksTimeout value ... If the hook procedure times out, the system passes the message to the next hook. However, on Windows 7 and later, the hook is silently removed without being called. There is no way for the application to know whether the hook is removed." And: "Windows 10 version 1709 and later The maximum timeout value the system allows is 1000 milliseconds."
* Microsoft's own advice on the same pages: "run the hooks on a dedicated thread that passes the work off to a worker thread and then immediately returns."

Design that follows:

1. One dedicated hook thread installs both hooks with `GetModuleHandleW(None)` as `hMod`, creates its queue with `PeekMessageW(PM_NOREMOVE)` before signaling ready, and runs a `GetMessageW` loop that handles the `-1` return.
2. The callbacks copy a fixed-size record into a preallocated lock-free queue, unpark the pipeline thread, and return `CallNextHookEx`. No allocation, no locks, no logging, never a nonzero return (Keystage only observes).
3. Key state is never read inside the callback ("the asynchronous state of the key cannot be determined by calling GetAsyncKeyState from within the callback function"); modifiers are tracked from the event stream and resynced with `GetAsyncKeyState` on unlock, resume and return from an elevated window.
4. A watchdog off the hook thread compares `GetLastInputInfo` with what the hook delivered and reinstalls the hooks if Windows removed them silently.
5. Pausing unhooks both hooks; stopping posts a private `WM_APP` message and the thread unhooks and calls `PostQuitMessage` itself (WM_QUIT is never posted directly: https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-quit).

Claims in the prompt that were wrong or imprecise, found here:

* `WM_MOUSEHWHEEL` is not in LowLevelMouseProc's documented list of messages, and `MSLLHOOKSTRUCT.mouseData` is documented only for vertical wheel and X buttons. The hook handles horizontal wheel defensively; it must be confirmed on hardware.
* AltGr's fake left Ctrl is documented by Microsoft only in an archived blog ("altGr generates a left ctrl + right alt key down combination", https://learn.microsoft.com/en-us/archive/blogs/murrays/hot-keys-and-altgr) and the PowerToys page; its scan code `0x21D` is undocumented (a third party calls it "not a documented value"). The core keeps both the scan code test and a same-timestamp fallback; this needs a check with a German or Spanish layout.
* `WS_EX_TRANSPARENT` alone does not make a window click-through; pass-through is documented for a layered window with that style (which is what tao sets).
* `WDA_EXCLUDEFROMCAPTURE` needs Windows 10 2004 (build 19041); on 1809 to 1909 Windows applies `WDA_MONITOR` and records an empty rectangle, so the "exclude from capture" option is disabled below build 19041.
* Microsoft says `GetDpiForMonitor` "should not be used if the calling thread is per-monitor DPI aware"; the app uses `GetDpiForWindow` where it can.

Windows 10 build 17763 is version 1809 (https://learn.microsoft.com/en-us/windows/release-health/release-information, read 2026-10-01). Every API above exists at that floor; `ToUnicodeEx` flag `0x4` (no keyboard state change) needs 1607.

### 8. Compile-checking the Windows code from this Linux container

**Yes. It is part of the local gate.**

Commands (2026-10-01):

```
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin --locked          # installed cargo-xwin 0.23.1
cargo xwin build --release --target x86_64-pc-windows-msvc   # a probe crate using the windows crate: built and linked xwin-probe.exe (125,952 bytes) in 1m04s, including the first download of the MSVC CRT and Windows SDK (1.1 GB cache)
cargo xwin check -p idea-keystage --target x86_64-pc-windows-msvc   # the Tauri app skeleton: Finished in 1m07s
npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis   # with system makensis v3.09 (apt package nsis 3.09-4ubuntu1): produced "IDEA Keystage_0.1.0_x64-setup.exe" (2,127,625 bytes) in 3m13s
```

The cross-built installer is unsigned and was not run (no Windows here). Tauri's guide calls NSIS cross-compilation "possible with caveats ... should only be used as a last resort" (https://v2.tauri.app/distribute/windows-installer/, read 2026-10-01), so the release that Mr. Pina downloads is built on a `windows-latest` GitHub runner, and the Linux cross-build is a local check only.
