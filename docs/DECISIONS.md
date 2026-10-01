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

| Requirement                 | How                                                                                                                                                                                                                                                   | Source                                                                                     |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Transparent                 | `transparent(true)`; wry sets the WebView2 background to (0,0,0,0); page sets `background: transparent`                                                                                                                                               | tao `window.rs`, wry `webview2/mod.rs`                                                     |
| No border or rounded shadow | `shadow(false)` (the default `true` gives an undecorated window a 1px border and Windows 11 rounded corners)                                                                                                                                          | Tauri config reference                                                                     |
| Always on top               | `always_on_top(true)` (`WS_EX_TOPMOST`, `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE)`), re-asserted on foreground change                                                                                                                               | tao `window_state.rs`                                                                      |
| Never takes focus           | `focused(false)` and `focusable(false)` (tao adds `WS_EX_NOACTIVATE`); as built, the first show is `SW_SHOW` with `WS_EX_NOACTIVATE` already set, because tao 0.37.1 clears its do-not-focus marker after creation (ARCHITECTURE.md section 6)        | tao `window_state.rs`                                                                      |
| Click-through               | `set_ignore_cursor_events(true)` (tao adds `WS_EX_TRANSPARENT \| WS_EX_LAYERED`)                                                                                                                                                                      | tao `window_state.rs`                                                                      |
| Not in the taskbar          | `skip_taskbar(true)` (`ITaskbarList::DeleteTab`)                                                                                                                                                                                                      | tao `window.rs`                                                                            |
| Not in Alt+Tab              | **raw Win32**: `WS_EX_TOOLWINDOW` on, `WS_EX_APPWINDOW` off, held in place by a `SetWindowSubclass` handler on `WM_STYLECHANGING`, because tao rewrites `GWL_EXSTYLE` from its own flags on every state change                                        | Microsoft extended window styles and taskbar pages; tao `window_state.rs` lines 418 to 431 |
| Per-monitor DPI v2          | **own application manifest** through `tauri_build::WindowsAttributes::app_manifest` (`src-tauri/app.manifest`), because tauri-build's default manifest declares only Common Controls v6 and tao sets PMv2 at runtime, which Microsoft advises against | tauri-build 2.7.1 `src/windows-app-manifest.xml`; tao `dpi.rs`; Microsoft DPI page         |
| HWND access                 | `WebviewWindow::hwnd()` returns `windows` 0.62 `HWND`, so the app pins `windows = 0.62.2`                                                                                                                                                             | docs.rs Windows target page                                                                |

What changed in the design because of this answer:

- The prompt assumed `WS_EX_NOACTIVATE` needs raw Win32. It does not: `focusable(false)` sets it. Raw Win32 is needed for Alt+Tab (`WS_EX_TOOLWINDOW`), and the styles need a `WM_STYLECHANGING` guard to survive tao.
- Host A is never hidden at the OS level after it is created. A raw `ShowWindow` call desynchronizes tao's visibility flag, and a later Tauri `show()` may activate the window. Hiding Host A (OBS sources in use, paused) is done by the renderer drawing nothing; turning it off for good closes the window.
- Host A and the settings preview receive events through Tauri events, not the WebSocket. Chromium's Local Network Access rules reached WebSockets in Edge 147 and WebView2 plans to make them API-controlled (https://github.com/MicrosoftEdge/WebView2Announcements/issues/126, read 2026-10-01), so keeping Tauri pages off the socket removes that risk. The server's Origin allowlist therefore stays limited to `http://127.0.0.1:<port>` and `http://localhost:<port>`.
- Prior art: Keyviz (https://raw.githubusercontent.com/mulaRahul/keyviz/main/src-tauri/tauri.conf.json, read 2026-10-01) is a Tauri 2 keystroke visualizer using the same window flags, which supports the choice. Its code was not used.

Pinned versions (read 2026-10-01 from https://index.crates.io and https://registry.npmjs.org; Cargo pins are exact `=x.y.z`, both lockfiles are committed):

| Rust crate                          | Version                                |     | npm package                                     | Version                                                      |
| ----------------------------------- | -------------------------------------- | --- | ----------------------------------------------- | ------------------------------------------------------------ |
| tauri                               | 2.12.1                                 |     | @tauri-apps/cli                                 | 2.12.1                                                       |
| tauri-build                         | 2.7.1                                  |     | @tauri-apps/api                                 | 2.12.1                                                       |
| tauri-plugin-single-instance        | 2.5.2                                  |     | @tauri-apps/plugin-global-shortcut              | 2.4.0                                                        |
| tauri-plugin-global-shortcut        | 2.4.0                                  |     | @tauri-apps/plugin-autostart                    | 2.7.0                                                        |
| tauri-plugin-autostart              | 2.7.0                                  |     | @tauri-apps/plugin-dialog                       | 2.8.1                                                        |
| tauri-plugin-dialog                 | 2.8.1                                  |     | @tauri-apps/plugin-clipboard-manager            | 2.4.1                                                        |
| tauri-plugin-clipboard-manager      | 2.4.1                                  |     | @tauri-apps/plugin-opener                       | 2.7.0                                                        |
| tauri-plugin-opener                 | 2.7.0                                  |     | svelte                                          | 5.57.1                                                       |
| windows                             | 0.62.2                                 |     | @sveltejs/vite-plugin-svelte                    | 7.3.1                                                        |
| axum                                | 0.8.9                                  |     | vite                                            | 8.3.1                                                        |
| tokio                               | 1.53.1                                 |     | svelte-check                                    | 4.7.6                                                        |
| tokio-tungstenite                   | 0.29.0 (what axum 0.8.9 `ws` requires) |     | typescript                                      | 6.0.3 (7.x is outside svelte-check's peer range)             |
| futures-util                        | 0.3.34                                 |     | vitest                                          | 5.0.3                                                        |
| serde / serde_json                  | 1.0.229 / 1.0.151                      |     | jsdom                                           | 29.1.1 (30.x needs Node 22.22.2; this container has 22.22.0) |
| crossbeam-channel / crossbeam-queue | 0.5.17 / 0.3.14                        |     | prettier / prettier-plugin-svelte               | 3.9.9 / 4.1.1                                                |
| parking_lot                         | 0.12.5                                 |     | @types/node                                     | 26.6.3                                                       |
| mime_guess                          | 2.0.5                                  |     | @fontsource/rajdhani, share-tech-mono, orbitron | 5.3.0 each                                                   |
| rodio                               | 0.22.2 (`playback`, `wav` only)        |     | playwright (scripts only)                       | 1.56.1, matching the installed Chromium 1194                 |
| base64                              | 0.22.1                                 |     |                                                 |                                                              |
| sha2                                | 0.11.0                                 |     |                                                 |                                                              |
| thiserror / anyhow                  | 2.0.21 / 1.0.104                       |     |                                                 |                                                              |
| tracing / tracing-subscriber        | 0.1.44 / 0.3.23                        |     |                                                 |                                                              |
| tempfile (tests)                    | 3.27.0                                 |     |                                                 |                                                              |

Toolchain: `rustc 1.97.0`, `cargo 1.97.0` (tauri 2.12.1 declares `rust-version = 1.90`), Node `v22.22.0`, npm `10.9.4`.

### 3. SolidWorks shortcuts and mouse controls

**Answer: the SolidWorks profile data is sourced only from Dassault's SOLIDWORKS Design Help on help.solidworks.com for 2025 and 2026, and lives in `data/profiles/solidworks.json`, where every entry carries its URL, version and date read (2026-10-01) plus a verbatim quote.** Two extraction agents read the help topics (keyboard and view orientation; mouse, middle button, wheel and gestures), then two independent verifiers re-fetched every cited page and removed or corrected anything the quote did not support. Final counts, the 2025 versus 2026 differences and the full gap list are below.

How the pages were read: help.solidworks.com is a Next.js site that answers plain `curl` with HTTP 403 (Akamai) but serves a topic, with a browser user agent, whose body sits in the page's `__NEXT_DATA__` JSON at `props.pageProps.helpContentData.helpText`. A helper script extracted that text for each topic URL, `https://help.solidworks.com/<version>/english/SolidWorks/<file>`. Starting topics: `sldworks/r_keyboard_shortcuts.htm` (Keyboard Shortcuts), `c_time_saving_keyboard_shortcuts.htm` (Selected Keyboard Shortcuts), `HIDD_CUSTOMIZE_KEYBOARD.htm`, `r_Middle_Mouse_Button.htm` (Middle Mouse Button Functions) and `c_mouse_sestures.htm` (Mouse Gestures; Dassault's filename is misspelled), then every linked topic.

**Counts (after verification, the second search and the review fix; read from the file 2026-10-01).** 364 keyboard entries and 183 mouse entries (547 in all), citing 294 source pages (139 for 2025, 155 for 2026): 284 in the core SOLIDWORKS help, 9 in the 2026 online tutorials (`swtutorialonline`) and 1 in the 2025 What's New. 525 entries appear in both versions, 12 only in 2026, 10 only in 2025 (8 of them the screenshot-sourced standard views below). The file also lists 31 differences and 17 gaps. The keyboard verifier checked 361 extracted entries, moved 2 whose quotes named no mouse action into the shortcut list, corrected 69 groups of fields (mostly quotes that did not contain the key or every word of the caption), and added 32 entries the extractor missed. The mouse verifier checked 191 entries, removed none, corrected 25 and added 20. Merging removed 36 duplicate shortcuts and 26 duplicate mouse entries that both agents had found. The first review round then removed three entries, `kb-020` to `kb-022` ("Alt + 0176", "Alt + 0216", "Alt + 0181", from "Alt + specified values | Inserts standard Windows symbols" in Selected Keyboard Shortcuts): the profile's step model read them as "Alt + 0, then bare 1, 7, 6", which is not the documented action, and they made `Alt + 0` a chord prefix. A step model cannot express a modifier held across several keys, so they stay under the gap "Pure keyboard rows not turned into entries". Their ids are retired; nothing was renumbered.

**2025 versus 2026.** No mouse binding that exists in both versions changed, and no gesture rule or view option changed; the 2026-only mouse entries (relation groups, 3DEXPERIENCE Alt + drag, the tutorial's 8-gesture mappings) are listed below. The keyboard differences are small and are all recorded in the data file's `differences` list (31 items). The ones a user could notice:

- Selection filters: the 2025 list of filter keys names E (edges), X (faces) and V (vertices); the 2026 list also names F6 (Toggle Selection Filters), which both versions document elsewhere on the page ("You can also press F6."), and notes more filters that can be assigned keys (`r_selection_filter.htm`).
- Autotransitioning: in both versions A switches between line and arc; 2026 adds that it is "Available when Enable on screen numeric input on entity creation is cleared" (`t_Autotransitioning.htm`).
- Search: 2026 adds "To find and run commands, click Search Commands or press S." (`c_search.htm`); W and S for search are in both.
- Relation groups (Ctrl + select) exist only in 2026; several 3DEXPERIENCE-only Alt + drag statements appear only in 2026.
- 2026 renames the product "SOLIDWORKS Design" throughout; the mouse gesture tutorial with named 8-gesture mappings exists only on the 2026 site.
- Smaller recorded differences: the 2026 Selected Keyboard Shortcuts row for Shift + arrow keys (Routing) adds "Not available for some 3DEXPERIENCE users."; Interrupt Regeneration lists more commands Esc can exit in 2026; FeatureManager options add a right-click rename beside F2 in 2026; Open dialog box's Ctrl+O sentence moved to a 2026-only topic; the Delete key for deleting an equation is documented only in 2025; several topics were renamed or moved between versions with the same shortcut text. The `differences` array holds all 31 with the topic file names.

**Gaps (no Dassault page for 2025 or 2026 documents these, so the data has no entry for them).** These are reported, not filled from memory. All 17 from the file's `gaps` array, each with the URL checked there:

1. **Ctrl + number standard view shortcuts** (Front, Isometric, Normal To and so on). Not documented in **text** for either version. For 2025 only, a What's New screenshot shows them (second search below); those eight are in the data as 2025 screenshot entries. For 2026 this remains a gap. Searched: Standard Views Toolbar, Orientation, Orientation Dialog Box, View Selector, View Toolbar, Heads-up View Toolbar, Selected Keyboard Shortcuts, the API `swCommands_e` list, and every help topic in both versions.
2. **A complete list of default keyboard shortcuts.** It is not published as a help topic; Dassault directs users to the Keyboard tab's Print List or Copy List. This is why the Copy List import (item 4) matters: it brings the user's own full list, including standard view keys, into the captions.
3. View topics with no keyboard shortcut at all: Previous View, Zoom to Area, Zoom to Selection, Zoom to Sheet, Viewports, Model View, Predefined Views, drawing View Orientation.
4. Walk-through speed number keys: "You can also press a number key to change the speed." names no keys, so no per-digit entries.
5. Access keys (underlined menu letters) other than the Alt+F example are not enumerated.
6. Repeated generic "hold Ctrl and select" and "press Delete" mentions across dozens of feature topics were not encoded one by one; the general entries cover them, and modifier selections on topics already cited were added.
7. Guides outside the core `sldworks` help (Simulation, Motion Studies, Routing, the AutoCAD-style help, Toolbox, the install guide) were not mined.
8. Pure mouse actions without a key in the keyboard extraction (middle-drag rotate, wheel zoom, right-drag gestures beyond the two Selected Keyboard Shortcuts rows). Out of scope for the keyboard file; the mouse research covered them separately.
9. Middle mouse button double-click (for example zoom to fit): not documented. The only match was SOLIDWORKS Treehouse, a separate application.
10. Which wheel direction zooms in by default: not documented (only the "Reverse mouse wheel zoom direction" option); wheel entries use action `wheel` and match both directions. The only match was eDrawings, a separate product.
11. Factory default mouse gesture assignments per document type: shown only as unlabeled images, byte-identical in 2025 and 2026; the 2026 online tutorial names a few 8-gesture mappings, which are in the data as 2026 only.
12. The 2025 online tutorials (`swtutorialonline`) do not exist on the 2025 site ("Product not found"), so tutorial-derived gesture entries carry only 2026.
13. A separate "Using Mouse Gestures" topic does not exist in 2025 or 2026; that content lives in the Mouse Gestures topic.
14. Gesture directions for 2, 3, 4 and 12 gesture guides are not documented in text (only that 2 gestures can be horizontal or vertical). The 2026 tutorial states the 8-gesture directions. The topic's "right-drag to the lower right" example names no tool, so no entry was made from it.
15. Pure keyboard rows not turned into entries: "Alt + specified values" symbol entry (the three removed `kb-020` to `kb-022`) and the walk-through number keys. The gap text also names the access-key sequence "Alt+F, then C" on Keyboard Shortcuts, but the merged file does carry it as `kb-003` (`Alt`, `F`, `C`, "Close the active document", from that same topic for both versions); the gap's sentence predates the merge. The entry is correct; the gap text is stale on that one point.
16. Triad: "To rotate an object about a ring's axis, drag the wing." reads as self-contradictory in both versions, so no entry; plain triad, handle and callout drags were not turned into entries either.
17. A right-click followed by a shortcut-menu or context-toolbar pick (Rotate about scene floor, Lock Camera, Magnified Selection, Zoom to Selection, OK or Cancel, and others): not entries, since the right-click itself is the "Shortcut menus" entry.

The verify notes add two facts that are not gaps but limit what the app can show: mouse entry `ms-020` (Ctrl + Alt + middle "press", Turn Camera) uses an action the matcher does not classify, so it is kept but never captioned; and `ms-012` and `ms-013` are drag or click entries with a direction, which are never matched because drags are not classified by direction.

**Second search for gaps 1, 9, 10 and 11** (an agent pattern-searched every topic in the help table of contents for both versions, 6,097 for 2025 and 6,251 for 2026, plus the full What's New sets, the 2026 tutorials, the API help, and 288 site searches; 15,452 page texts in all, 2026-10-01):

- Gap 1 is **closed for 2025 only, by a screenshot.** Dassault's What's New topic "Search Commands (2025 SP2)" (https://help.solidworks.com/2025/english/WhatsNew/c_ui_search_commands.htm) says "When you use Search Commands, the results include the tools' keyboard shortcuts in parenthesis if they exist." and shows a screenshot (https://help.solidworks.com/2025/english/WhatsNew/krt1738080155112.image, SHA-256 `150a8aed109ea30d5984964e9f2b10aef80bbc254fa5ce97df94556a5b9835c8`) whose labels read "Front (Ctrl+1)", "Back (Ctrl+2)", "Left (Ctrl+3)", "Right (Ctrl+4)", "Top (Ctrl+5)", "Bottom (Ctrl+6)", "Isometric (Ctrl+7)", "Normal To (Ctrl+8)". Two independent readers (the agent, then the orchestrator viewing the downloaded PNG) read the same labels, and the same screenshot's "View Orientation (SpaceBar)" and "View Selector (Ctrl+SpaceBar)" agree with shortcuts the help states in text. **Decision: these eight are in the data as version 2025 entries marked `"evidence": "screenshot"`**, with the image URL and hash in their source. This is image evidence, not page text; Mr. Pina can remove them by deleting the eight `kb-360` to `kb-367` entries. No 2026 page documents them in text or image, so for 2026 they remain a gap; the app shows the caption whenever the SolidWorks profile is active, because it cannot tell 2025 from 2026.
- Gaps 9 (middle double-click), 10 (wheel direction) and 11 (default gestures) remain. The only matches were for separate products (SOLIDWORKS Treehouse for a middle double-click "zoom extents", eDrawings for the wheel direction), which do not apply to SOLIDWORKS document windows.

What changed in the design because of this answer:

- Captions are shown only for sourced entries. Because many sourced entries are context-specific (the same key does different things in a sketch, an assembly or a dialog), each caption carries its context in small type, and an entry with no stated context wins a tie.
- "drag" or "click" with no button named in the documentation is kept as button `unspecified` in the data. The code matches it to a left-button drag or click. That is an interpretation, not a documented fact (the pages say "drag" without naming a button), and the code says so where it does it.
- The Copy List import became the way to get complete captions, since the documentation itself is incomplete.

### 4. Exporting and reading customized keyboard shortcuts

**Decision: the primary import is the Keyboard tab's Copy List (pasted text or a saved `.txt`). The `.sldreg` import is experimental. Both are unvalidated until Mr. Pina supplies a real export from SolidWorks 2025 and 2026.**

What Dassault documents (all read 2026-10-01, identical in substance for 2025 and 2026 unless noted):

- The tool is the **SOLIDWORKS Settings Wizard** (Tools > Save/Restore Settings). "Keyboard shortcuts" is one of six categories it can save: "You can save or restore system settings for: System options Toolbar layout (All toolbars or Macro toolbar only) Keyboard shortcuts Mouse gestures Menu customization Saved views" (https://help.solidworks.com/2025/english/SolidWorks/sldworks/c_solidworks_settings_wizard.htm and the 2026 page). "The *.sldreg file is generated by the SOLIDWORKS Settings Wizard" (https://help.solidworks.com/2025/english/Installation/install_guide/t_settings_options_welcome.htm and 2026). The API equivalent is `ISldWorks::SaveSettings`, "Saves the specified SOLIDWORKS settings to the specified *.sldreg file" (https://help.solidworks.com/2025/english/api/sldworksapi/SolidWorks.Interop.sldworks~SolidWorks.Interop.sldworks.ISldWorks~SaveSettings.html and 2026).
- The settings are saved "to a file" and restored "to the following registries: ... CURRENT_USER of current user ... LOCAL_MACHINE of selected computers" (Settings Wizard page). **No 2025 or 2026 page states the `.sldreg` file format, its encoding, whether it is a `.reg` text export, or any registry key path for keyboard shortcuts.** The research searched the Design Help, the whole Installation and Administration guide (157 topics per version, walked in table-of-contents order), the API help and the 2026 tutorials.
- The Keyboard tab of Tools > Customize documents two more exports: "Copy List: Click to copy the selected list to the clipboard so you can paste it into documents such as Word or Excel" and "Print List: ... print the selected list with columns for Category, Command, and Shortcut(s)"; also "Letters are shown in uppercase" and that assigning a shortcut already in use removes it from the old command (https://help.solidworks.com/2025/english/SolidWorks/sldworks/HIDD_CUSTOMIZE_KEYBOARD.htm and 2026). "Print or copy a list of shortcut keys from the Keyboard tab of the Customize dialog box" (https://help.solidworks.com/2025/english/SolidWorks/sldworks/r_keyboard_shortcuts.htm and 2026).

Claims in the prompt that were wrong:

- "Copy Settings Wizard": that was the name through the 2022 help; the 2023 help renamed it the SOLIDWORKS Settings Wizard (https://help.solidworks.com/2022/english/SolidWorks/sldworks/c_copy_settings_wizard.htm versus https://help.solidworks.com/2023/english/SolidWorks/sldworks/c_solidworks_settings_wizard.htm).
- ".sldreg is a registry export": not stated by any 2025 or 2026 page. It may be one; it is not documented.
- ".sldreg is the expected route": the Copy List is documented better (its purpose and its three columns), so it is the primary route.

What was built from this:

- `Copy List` parser: tab, comma or semicolon delimiter detection; optional header row; UTF-8 with or without BOM and UTF-16LE with BOM; CRLF or LF; several shortcuts in one cell; lowercase letters normalized; unknown keys and malformed rows kept as warnings; duplicate bindings warned. Tested against fixtures constructed from the documented columns, with synthetic command names (`Fixture Command One`), never real SolidWorks names.
- `.sldreg` sniffer: empty, binary, and Registration Entries text (UTF-8 or UTF-16LE) are recognized and tokenized, but with no documented keyboard section it reports "could not read the keyboard section; use Copy List" rather than guess. Its fixtures are labeled hypothesis fixtures.
- Both routes show a preview of added and changed captions before anything is applied, and imported entries are tagged as coming from the user's own SolidWorks.

Validation still required (Mr. Pina, once): with SolidWorks 2025 and 2026, save a `.sldreg` with only "Keyboard shortcuts" selected, and paste a Copy List of "All Commands" into Notepad. With those two files the parsers can be validated and the hypothesis fixtures replaced.

### 5. OBS integration surfaces

**Browser Source** (https://obsproject.com/kb/browser-source, read 2026-10-01, page dated 2022-01-11; behavior details from the obs-browser source at commit `3f0a2cdf378939ebe3c6f9ab36d4ea100c25aac2`, the one pinned by OBS 32.2.2, https://raw.githubusercontent.com/obsproject/obs-browser/3f0a2cdf378939ebe3c6f9ab36d4ea100c25aac2/obs-browser-plugin.cpp, read 2026-10-01):

- Width and Height default to 800 and 600 (range 1 to 8192). The default Custom CSS is `body { background-color: rgba(0, 0, 0, 0); margin: 0px auto; overflow: hidden; }`, injected as a `<style>` after the page loads, so it overrides page rules of equal specificity. "Shutdown source when not visible" unloads the page; "Refresh browser when scene becomes active" reloads it when the source becomes active; both default off. "Use custom frame rate" defaults off on Windows (the page renders at the canvas frame rate).
- Inside OBS, `window.obsstudio` exists (obs-browser README), which is how the overlay knows it is in OBS.
- Loading `http://127.0.0.1` or `http://localhost` is **not explicitly documented**. The obs-browser source has no host filter for the URL field, and a 2018 forum thread reports loopback working (community evidence only). This must be confirmed on Mr. Pina's OBS.
- A "Local file" page gets the origin `http://absolute`, so Keystage sources use URL mode with `http://127.0.0.1:<port>/overlay/<route>`.
- Embedded Chromium: OBS 30.x uses CEF branch 5060 (Chromium **103**), OBS 31.x and 32.x use branch 6533 (Chromium **127**). OBS documents only the 31.0.0 change ("Updated the browser source/docks CEF (Chromium) to version 127 (6533)", release notes); the others come from `buildspec.json` / `CMakePresets.json` at each tag on raw.githubusercontent.com mapped through https://chromiumdash.appspot.com/fetch_milestones (read 2026-10-01).

**obs-websocket 5** (https://raw.githubusercontent.com/obsproject/obs-websocket/master/docs/generated/protocol.md and the README, read 2026-10-01; byte-identical to the copy pinned by OBS 32.2.2):

- Built into OBS 28 and later. Default port 4455 (README). The server is **disabled by default** and authentication is on by default with a generated 16-character password (`src/Config.h`, `Config.cpp`). The menu item is Tools > WebSocket Server Settings ("Enable WebSocket server", "Show Connect Info").
- Connection: server `Hello` (op 0, with `authentication.challenge` and `salt`), client `Identify` (op 1, `rpcVersion: 1`, `authentication`, `eventSubscriptions`), server `Identified` (op 2), then `Request` (op 6) / `RequestResponse` (op 7) and `Event` (op 5).
- Authentication, verbatim from the protocol: concatenate password and salt, SHA-256, base64 (the "base64 secret"); concatenate that with the challenge, SHA-256, base64. The challenge is per connection, so it is recomputed every time. The protocol's own Identify example does not match its Hello example; the client is tested with a vector computed from the documented algorithm.
- Subscriptions: `eventSubscriptions: 65` (General 1 + Outputs 64). `RecordStateChanged` is in Outputs; its `outputState` is one of `OBS_WEBSOCKET_OUTPUT_STARTING|STARTED|STOPPING|STOPPED|PAUSED|RESUMED` for recording, and `outputActive` is false while paused, so state is derived from `outputState`.
- `GetRecordStatus` returns `outputActive`, `outputPaused`, `outputTimecode` (`HH:MM:SS.mmm`), `outputDuration` (milliseconds) and `outputBytes`. Keystage seeds state with it after `Identified` and polls it each second while recording to map its own clock to recording time.
- `CreateRecordChapter` exists from obs-websocket 5.5.0 (OBS 30.2) and works only with Hybrid MP4 (and Hybrid MOV from OBS 32.0). It fails with code 702 for MKV, which is the default container in OBS 30.2 and 31 release builds. Keystage therefore always records chapter markers in its own Session Log and mirrors them into OBS only when supported.
- Close code 4009 means authentication failed (stop retrying until the password changes); 4011 means kicked (do not reconnect automatically).

What changed in the design because of this answer:

- The overlay is built to a **Chromium 103** floor (`build.target` and `build.cssTarget` `chrome103`) so the same sources work in OBS 30, 31 and 32. Overlay CSS avoids `:has()`, `@container`, `color-mix()`, `oklch()`, nesting and other features newer than 103 unless a finished-looking fallback exists.
- The overlay sets its own transparent, margin-free, full-size root, because OBS's Custom CSS is user-editable and centers the body with `margin: 0px auto`.
- The server sends a full snapshot on every connect, since OBS sources reload on scene activation when that option is on.
- The settings window tells the user to enable the WebSocket server in OBS first, and suggests Hybrid MP4 when chapter markers should also land in the recording file.

Claims in the prompt that were wrong or imprecise: the OBS knowledge base still says "obs-websocket Settings" where the current menu says "WebSocket Server Settings"; "Control audio via OBS" is not in the KB's property table (it is in the source); the protocol document does not state the default port (the README does).

### 6. Password fields and the foreground process

**Password fields: two checks, fail closed.**

- UI Automation: `UIA_IsPasswordPropertyId` (30019) "indicates whether the automation element contains protected content or a password. When the IsPassword property is TRUE and the element has the keyboard focus, a client application should disable keyboard echoing or keyboard input feedback" (https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-automation-element-propids, read 2026-10-01). Edit controls "Must be set to TRUE on edit controls that contain passwords" (https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supporteditcontroltype), so detection is only as good as the target app's accessibility support.
- Threading: UIA calls belong on "a separate thread. ... This thread should not own any windows, and should be a ... Multithreaded Apartment (MTA) model thread" (https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading). Property reads are cross-process ("slow and inefficient because it requires a cross-process call for each property", https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients), so the design subscribes once with `AddFocusChangedEventHandler` and a cache request for IsPassword, on its own MTA thread, and reads the cached value in the handler. Implementing the handler needs `windows-core` as a direct dependency (`#[implement]`).
- Cheap Win32 check for classic controls: `GetGUIThreadInfo` (its `hwndFocus`, https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getguithreadinfo) plus class `Edit` and the `ES_PASSWORD` style (https://learn.microsoft.com/en-us/windows/win32/controls/edit-control-styles). It sends no message to the target, so it cannot hang, but browser, Electron, WPF and UWP password boxes are not separate HWNDs and need UIA.
- Fail closed: while either check says password, nothing is shown, logged or sounded. From every focus or foreground change until UI Automation has classified the new focus, the input receiver holds keys back. "Not a password" hands them out in order. Anything else drops them: a password, an element that cannot be read (retried 3 times, 250 ms apart, and suppressed meanwhile), no answer within 400 ms (3 s while UI Automation starts; the app then suppresses until the answer), or a newer focus change. UI Automation calls are bounded to 2 s (`IUIAutomation2` ConnectionTimeout and TransactionTimeout; Microsoft's defaults are 2 s and 20 s, https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-put_transactiontimeout and the matching put_ConnectionTimeout page, read 2026-10-01). Keys typed right after a Tab, Enter, Esc, F6 or a mouse click are kept back until the watcher reports where the focus went (at most 150 ms when it reports nothing), and follow that focus's classification. The `Password` event is always sent before the hold changes. A hold that ends without the app suppressing drops modifiers and Caps Lock too. The 150 ms bound is a judgment, not a measured or documented figure; an app that takes longer to move the focus after a Tab leaves the keys typed in that time to the previous focus's state. Verify with KeePass auto-type on a loaded machine.
- Ordering: a context change that hides applies before the input drained with it; a change that shows more applies only after the input of that pipeline pass and the next one. While UI Automation password detection is not running (at start, or for good if it fails), typing is masked in every profile and a notice says so.

**Foreground process: on every foreground change, cheaply.**

- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, ..., WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)`; "The client thread that calls SetWinEventHook must have a message loop in order to receive events" and out-of-context events are "delivered on the same thread that called SetWinEventHook" (https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook, read 2026-10-01). It runs on a dedicated watcher thread, never the hook thread.
- Name lookup: `GetWindowThreadProcessId`, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`, `QueryFullProcessImageNameW`, `CloseHandle`, cached per process id (https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew, https://learn.microsoft.com/en-us/windows/win32/procthread/process-security-and-access-rights). A NULL foreground window or an `OpenProcess` failure is "unknown", never "not excluded".
- No Microsoft page states whether a medium-integrity process can query an elevated one. While an elevated window is foreground the hooks most likely see no input at all (UIPI, https://learn.microsoft.com/en-us/previous-versions/dotnet/articles/bb625963(v=msdn.10)), so the overlay shows nothing stale and resyncs when focus returns.

### 7. Low-level hook constraints

Both claims are confirmed by Microsoft (https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc and https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc, read 2026-10-01):

- Message loop: "This hook is called in the context of the thread that installed it. The call is made by sending a message to the thread that installed the hook. Therefore, the thread that installed the hook must have a message loop."
- Timeout: "The hook procedure should process a message in less time than the data entry specified in the LowLevelHooksTimeout value ... If the hook procedure times out, the system passes the message to the next hook. However, on Windows 7 and later, the hook is silently removed without being called. There is no way for the application to know whether the hook is removed." And: "Windows 10 version 1709 and later The maximum timeout value the system allows is 1000 milliseconds."
- Microsoft's own advice on the same pages: "run the hooks on a dedicated thread that passes the work off to a worker thread and then immediately returns."

Design that follows:

1. One dedicated hook thread installs both hooks with `GetModuleHandleW(None)` as `hMod`, creates its queue with `PeekMessageW(PM_NOREMOVE)` before signaling ready, and runs a `GetMessageW` loop that handles the `-1` return.
2. The callbacks copy a fixed-size record into a preallocated lock-free queue, unpark the pipeline thread, and return `CallNextHookEx`. No allocation, no locks, no logging, never a nonzero return (Keystage only observes).
3. Key state is never read inside the callback ("the asynchronous state of the key cannot be determined by calling GetAsyncKeyState from within the callback function"); modifiers are tracked from the event stream and resynced with `GetAsyncKeyState` on unlock, resume and return from an elevated window.
4. A watchdog off the hook thread compares `GetLastInputInfo` with what the hook delivered and reinstalls the hooks if Windows removed them silently (input more than 3 s after the last record, at most once per 10 s; also when the hooks are missing for 2 s while not paused). As built, the hook thread also runs at `THREAD_PRIORITY_TIME_CRITICAL`, and its own 50 ms thread timer reinstalls both hooks after a stall of 150 ms or more, which also covers a lost keyboard hook while the mouse hook keeps delivering.
5. Pausing unhooks both hooks; stopping posts a private `WM_APP` message and the thread unhooks and calls `PostQuitMessage` itself (WM_QUIT is never posted directly: https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-quit).

Claims in the prompt that were wrong or imprecise, found here:

- `WM_MOUSEHWHEEL` is not in LowLevelMouseProc's documented list of messages, and `MSLLHOOKSTRUCT.mouseData` is documented only for vertical wheel and X buttons. The hook handles horizontal wheel defensively; it must be confirmed on hardware.
- AltGr's fake left Ctrl is documented by Microsoft only in an archived blog ("altGr generates a left ctrl + right alt key down combination", https://learn.microsoft.com/en-us/archive/blogs/murrays/hot-keys-and-altgr) and the PowerToys page; its scan code `0x21D` is undocumented (a third party calls it "not a documented value"). The core keeps both the scan code test and a same-timestamp fallback; this needs a check with a German or Spanish layout.
- `WS_EX_TRANSPARENT` alone does not make a window click-through; pass-through is documented for a layered window with that style (which is what tao sets).
- `WDA_EXCLUDEFROMCAPTURE` needs Windows 10 2004 (build 19041); on 1809 to 1909 Windows applies `WDA_MONITOR` and records an empty rectangle, so the "exclude from capture" option is disabled below build 19041.
- Microsoft says `GetDpiForMonitor` "should not be used if the calling thread is per-monitor DPI aware"; the app uses `GetDpiForWindow` where it can.

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

With the finished app the same command gave `IDEA Keystage_0.1.0_x64-setup.exe`, 4,245,631 bytes, in 191 s on a warm cache (cargo 2 min 59 s, LTO with one codegen unit); `idea-keystage.exe` is 16,420,352 bytes and `llvm-readobj` shows the Windows GUI subsystem and the embedded `PerMonitorV2` manifest. The Windows test binary also runs under Wine 9.0 (`cargo xwin test ... --no-run`, then `wine64` under `xvfb-run`) as a smoke test; Wine differs from Windows in four places the platform notes list (`MAPVK_VK_TO_VSC_EX` prefixes, `ToUnicodeEx` for numpad keys, the 0x56 key on the US table, DPAPI MAC checks), so the Windows tests stay strict and run for real on `windows-latest` in CI.

## Design changes driven by Phase 1

Each change below came from a Phase 1 answer that contradicted or sharpened the original plan.

| Phase 1 finding                                                                                  | What changed in the design                                                                                                                                           | Item |
| ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| tao rewrites `GWL_EXSTYLE` on every state change and Tauri cannot keep a window out of Alt+Tab   | A `SetWindowSubclass` guard on `WM_STYLECHANGING` holds `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`, `WS_EX_TRANSPARENT` and `WS_EX_LAYERED` on and `WS_EX_APPWINDOW` off | 2    |
| A raw `ShowWindow` desynchronizes tao, and a later `show()` may activate                         | Host A is never hidden at the OS level; the renderer draws nothing instead                                                                                           | 2    |
| tauri-build's default manifest declares no DPI awareness                                         | The app ships its own manifest with `PerMonitorV2`                                                                                                                   | 2    |
| Local Network Access rules reached WebSockets in Edge 147                                        | Host A and the settings preview use Tauri events, never the socket                                                                                                   | 2    |
| The Copy List is better documented than `.sldreg`, and no page documents the `.sldreg` format    | Copy List is the primary import; `.sldreg` is a sniffer that refuses to guess                                                                                        | 4    |
| No page documents Ctrl + number standard views in text, and no complete default list exists      | Captions come only from sourced entries; the 2025 screenshot entries are marked `evidence: screenshot`; the Copy List import is how a user gets complete captions    | 3    |
| Many documented keys are context-specific                                                        | Each caption carries its context in small type; an entry with no context wins a tie                                                                                  | 3    |
| "drag" with no button named                                                                      | Kept as button `unspecified` and matched to the left button, labeled as an interpretation in code                                                                    | 3    |
| OBS 30 embeds Chromium 103                                                                       | The overlay builds to `chrome103` and avoids newer CSS and JS                                                                                                        | 5    |
| OBS injects Custom CSS that centers the body                                                     | The overlay sets its own transparent, margin-free root                                                                                                               | 5    |
| OBS can reload or destroy a source on scene changes                                              | The server sends a full `hello` on every connect; the transport reconnects fast and silently inside OBS                                                              | 5    |
| `CreateRecordChapter` fails with code 702 on MKV, the default container                          | Chapter markers always go to the Session Log and are mirrored to OBS only when it can take them; the app suggests Hybrid MP4                                         | 5    |
| `outputActive` is false while paused                                                             | Record state comes from `outputState`                                                                                                                                | 5    |
| UI Automation calls are cross-process and need an MTA thread with no windows                     | One cached focus subscription on its own MTA thread, plus a request-and-answer path with bounded timeouts                                                            | 6    |
| A focus change and its UI Automation verdict arrive later than the keys typed into the new field | The password hold word: keys are held from each focus change until the verdict and dropped unless it says "not a password"                                           | 6    |
| Low-level hooks are silently removed after a timeout                                             | Hook callbacks only copy a record; a watchdog and a stall probe reinstall the hooks                                                                                  | 7    |
| `GetAsyncKeyState` cannot be read inside the hook                                                | Modifiers are tracked from the stream and resynced only outside the hook                                                                                             | 7    |
| `WDA_EXCLUDEFROMCAPTURE` needs build 19041 and Tauri applies `content_protected` unconditionally | Capture exclusion goes only through a build-gated platform call; the option is disabled below 19041                                                                  | 7    |
| NSIS cross-compilation is "a last resort"                                                        | Releases are built on `windows-latest`; the Linux cross build is a local check                                                                                       | 8    |

## Build decisions

Decisions made while building, with the reason for each. Facts from code are as of commit `5b3eee5` plus this documentation pass.

**Identity and packaging**

- Identifier `com.ideabosco.keystage`, product name "IDEA Keystage", binary `idea-keystage`, publisher "IDEA Engineering Pathway", version 0.1.0 (one version in `package.json`, `Cargo.toml` and `tauri.conf.json`, checked by `tests/unit/versions.test.ts`). Settings live in `%APPDATA%\com.ideabosco.keystage`, the log in `%LOCALAPPDATA%\com.ideabosco.keystage\logs`, sessions in `Documents\IDEA Keystage\Sessions`.
- NSIS only, `installMode: currentUser` (no administrator rights, installs under `%LOCALAPPDATA%`), `webviewInstallMode: downloadBootstrapper` silent (the installer fetches WebView2 only if it is missing). English only.
- The release workflow runs on a `v*` tag, runs the whole CI on that commit first, refuses a tag that differs from `tauri.conf.json`, builds with `--locked` and no cache, and publishes `IDEA-Keystage_<version>_x64-setup.exe`, `IDEA-Keystage_<version>_x64-portable.exe` (the plain release exe) and `SHA256SUMS.txt`. Tauri names the installer with a space, which GitHub rewrites in asset names, so it is renamed.
- No code signing (no certificate) and no updater: Tauri's updater "needs a signature to verify that the update is from a trusted source. This cannot be disabled." (https://v2.tauri.app/plugin/updater/, read 2026-10-01), and an update check would break privacy rule 7. Both are in `docs/ROADMAP.md`.
- The Rust toolchain is pinned to 1.97.0 in both workflows, because Rust 1.98.0 added a clippy lint that fails `-D warnings` on unrelated code (CI run 10, 2026-10-01).

**Crates and hosts**

- Workspace split: `keystage-core` (pure: events, processor, profiles, imports, settings, protocol, sound synthesis), `keystage-session` (pure: Session Log, recording clock, exports), `keystage-server` (axum, hub, routes, demo binary), `keystage-obs` (obs-websocket 5 client), and the Tauri app in `src-tauri`. `default-members` leave out `src-tauri`, so `cargo test` on Linux never builds Tauri. The session crate was split from the server so exports and their golden tests need no network code.
- Port 47820 on `127.0.0.1`, configurable. A busy port is reported in plain words (`PortInUse`), and a port inside a Windows excluded range has its own message (`PortBlocked`). At most 64 WebSocket clients.
- Host A and the settings preview get protocol messages as Tauri events (`keystage://overlay`), serialized once and filtered per window; OBS and browsers get the same JSON over the WebSocket. Host A windows are labeled `overlay-<n>`, built hidden, guarded, then shown once.
- Each command has its own Tauri permission; Host A can call only `overlay_hello`. Four commands were added to the contract: `overlay_hello`, `hotkeys_capture`, `hotkeys_status`, `platform_info`. `Status` gained `notices`, `session_export` returns `warnings`, and the app emits `keystage://settings` after every stored change.
- The app CSP keeps `style-src 'self' 'unsafe-inline'` exact with `dangerousDisableAssetCspModification: ["style-src"]`, because Tauri's nonce would make browsers ignore `'unsafe-inline'`.

**Privacy and input**

- An unidentified foreground (before the watcher's first report, or when `OpenProcess` fails) is suppressed and reported as `excluded`; an elevated foreground releases all keys. The app's own windows always use the general profile, so typing in settings is always masked.
- The password hold word, the 150 ms provisional hold after a focus-moving key or click, the fail-closed ordering of context changes, masking everywhere while UI Automation is not running, and the panic guard (rebuild up to three times, then stop showing input) all came from the review rounds; see item 6 and ARCHITECTURE.md sections 1 and 8.
- Key sounds follow displayed key-down messages only, so masked typing makes no sound and leaks no rhythm.
- Plain left clicks, double clicks and drags make no callout by default; every SolidWorks selection would otherwise be one.

**SolidWorks profile and imports**

- Copy List import (pasted or a `.txt`): delimiter and header detection, UTF-8 with or without BOM, UTF-16LE with BOM, several shortcuts per cell, warnings for unknown keys and duplicate bindings. Applying replaces `profiles.solidworks_user_shortcuts` with the import's entries (a Copy List is a full list); imported entries carry `source: "user-import"` and win over the data file for the same keys. Every import reports `validated: false` until a real export confirms the parser.
- `.sldreg`: sniffed (empty, binary, Registration Entries text in UTF-8 or UTF-16LE) and refused with "use Copy List" rather than guessed.
- SolidWorks mode Auto, On, Off (the hotkey steps through them). Typed values on by default in the SolidWorks profile; `profiles.solidworks_display` can turn typed values or mouse callouts off.

**Look and sound**

- Six themes (IDEA, Studio, Graphite, Blueprint, Carbon, Glass) plus a custom theme with four colors (accent, face, ink, glow); every text pair is measured at 4.5:1 or more over white, `#d3d6da`, `#262a2e` and black, and the custom theme moves an ink in lightness only until it passes. Tokens and the plate recipe are copied from `pina-hash/idea-app` at `8d703f514646282245c8682b1d7c4cd469ece4b3` (see `docs/SOURCES.md`).
- Six keyboard layouts (full, tenkeyless, 75 percent, 60 percent, laptop, and a left-hand CAD zone), ANSI and ISO; ISO is switched on automatically the first time the 102nd key arrives, since `OverlayConfig` has no ISO field.
- Key sounds are synthesized in `keystage-core/src/synth.rs` from filtered noise and damped modes: 12 packs, 4 variants each, 48 kHz mono, under 120 ms. No recordings and no third-party audio. Off by default; no audio device is opened while off.
- Settings window 1200 x 800 (minimum 980 x 680); below 1180 px wide it hides the preview column.

**Exports**

- SRT: UTF-8 without a BOM and CRLF line endings (a choice; SubRip has no encoding standard per the Library of Congress format page). Cues never overlap and last at least 1 s. YouTube chapters start at `00:00` and warn under three chapters or for a chapter under 10 s, per YouTube Help (https://support.google.com/youtube/answer/9884579, read 2026-10-01).

**Known item not fixed**

- After a password hold is discarded, an odd number of Caps Lock toggles typed inside it can leave the letter case of later typed-value readouts inverted until the next modifier resync. Display only, not privacy.

## Review rounds

Three review rounds ran with an adversarial verifier per dimension: early (logic, Windows: 8 findings confirmed, 3 refuted), second (privacy, contract, carryover: 11 confirmed, 7 refuted, including a blocker: the password gate was open between a focus change and the UI Automation verdict), and third (UX, release, privacy, carryover: 13 confirmed). Every confirmed finding was fixed except the Caps Lock display item above.

CI: run 10 (commit `09f8cd6`) failed on Rust 1.98.0's new clippy lint and on two Windows platform tests (injected navigation keys without the E0 prefix; the topmost bit on the hosted runner). Run 14 (commit `6ba044a`) passed both jobs; on `windows-latest` the release build and 202 app and platform tests passed, including an F24 key injected with `SendInput` through the real `WH_KEYBOARD_LL` hook, the watcher, the UI Automation subscription and a DPAPI round trip.

## Performance

**Measured** (all on this Linux container: 4 vCPU Intel Xeon at 2.10 GHz, 15.7 GB, kernel 6.18.44; 2026-10-01):

| What                                                                           | Instrument                                                                                                                                                                                                                                                                                                                                                                    | Result                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Core processor throughput                                                      | `cargo run --release -p keystage-core --example throughput` (`std::time::Instant` around 1,000,000 synthetic events, release profile with LTO)                                                                                                                                                                                                                                | General profile 75 ns per event (13.4 million events per second); SolidWorks profile 157 ns per event (6.3 million per second)                                                                                                                                                                                                                                                           |
| Burst correctness, core                                                        | `crates/keystage-core/tests/burst.rs`: 200 down and up pairs at 20 per second, five seeds per profile                                                                                                                                                                                                                                                                         | Every displayable key-down produced a callout in the same call; counts add up exactly; 0 dropped                                                                                                                                                                                                                                                                                         |
| Burst correctness, renderer                                                    | `src/lib/renderer` store and stack tests with fake timers, 200 messages at 20 per second                                                                                                                                                                                                                                                                                      | 0 dropped                                                                                                                                                                                                                                                                                                                                                                                |
| Host B end to end                                                              | `npm run perf` (`scripts/perf.mjs`): the real `keystage-server` demo binary with the real core sending `burst` at 20 keys per second for 10 s to `/overlay/keys` at 1920 x 1080 in headless Chromium 141.0.7390.37 driven by Playwright 1.56.1; perf stamps (server Unix ms) paired in the page with a `MutationObserver` and `requestAnimationFrame` probe on the same clock | 200 sent, 200 rendered, 0 dropped, 0 long tasks                                                                                                                                                                                                                                                                                                                                          |
| Host B latency (results.json, run 2026-10-01 14:51 UTC), ms median / p95 / max | same                                                                                                                                                                                                                                                                                                                                                                          | Send to WebSocket delivery 0.79 / 1.12 / 6.63; to DOM updated 1.40 / 2.42 / 7.63; to first animation frame after the DOM change 6.56 / 8.07 / 19.83; to the next frame (paint approximation) 23.19 / 24.36 / 35.83; page only, delivery to first frame 5.80 / 7.10 / 13.20. Two earlier runs gave first-frame medians of 9.5 and 10.6 ms and paint-approximation medians of 26 and 27 ms |
| Resources (results.json), percent of one core                                  | `/proc` stat, status and smaps_rollup sampled during 60 s idle and the burst                                                                                                                                                                                                                                                                                                  | Demo server: idle 0 percent, burst 2.1 percent mean, RSS 6 MiB. Overlay page renderer: idle 0.1 percent mean, RSS 123 MiB (PSS 62 MiB); burst 46.3 percent mean (68.1 percent peak interval), RSS 220 MiB (PSS 155 MiB). GPU process burst 14.1 percent; browser process burst 2.5 percent                                                                                               |
| Idle frames                                                                    | `.work/shots/visuals/checks.mjs` in Chromium                                                                                                                                                                                                                                                                                                                                  | An idle cursor and an idle keyboard draw zero animation frames; 4K spotlight frame script p95 0.40 ms                                                                                                                                                                                                                                                                                    |

The "paint approximation" is the second animation frame after the DOM change. Headless Chromium has no display, so it is not the time pixels reach a screen. The latest run counted one page error (the harness stores the count, not the text); the earlier runs reported none.

**Not measured** (no Windows, no OBS here):

- Host A latency (WebView2 with Tauri events) and OBS latency (CEF Chromium 103 or 127, OBS frame pacing).
- The app's CPU and memory on Windows, the cost of the hook procedure on real hardware (designed to be well under a microsecond: one atomic load, a 40-byte copy, a lock-free push, an unpark), the delay the password hold adds in normal fields (expected to be one `GetFocusedElement` round trip), and the 150 ms provisional hold in practice.
- Sound latency through rodio on a real audio device.
