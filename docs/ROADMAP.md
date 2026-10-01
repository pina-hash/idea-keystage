# Roadmap

What version 0.1.0 leaves for later, and why. Nothing in sections 2 to 6 is built; section 1 lists built features that still need checking on Windows. Items marked "needs Windows" can only be closed on Mr. Pina's machine or a Windows test machine.

## 1. Validate with real Windows, OBS and SolidWorks (first)

These are built but could not be checked from the Linux build container. The first-run checklist in `docs/history/2026-10-01-keystage-v1.md` covers the most important ones.

- Host A over SolidWorks: transparent, click-through, never takes the focus, not in Alt+Tab or the taskbar, stays on top. Physical placement on mixed-DPI monitors and after a scale change.
- Password fields: UI Automation `IsPassword` in Chrome, Edge, Electron, WPF, UWP and the app's own settings window; KeePass auto-type (`{USERNAME}{TAB}{PASSWORD}{ENTER}`) into a browser login form while a build runs, with nothing of the password, and no Shift or Caps Lock, on Host A, in OBS or in a Session Log; the delay of keys typed right after Enter or Esc in SolidWorks.
- Hooks: real `LowLevelHooksTimeout` removal and the watchdog and stall probe that reinstall them; AltGr on a German or Spanish layout; Num Lock, Pause and Print Screen codes; horizontal wheel; legends on AZERTY and QWERTZ.
- OBS: loopback URLs in a Browser Source (not documented by OBS), the overlay CSP in OBS 30 (Chromium 103) and OBS 31 or 32 (Chromium 127), authentication against a real obs-websocket, `CreateRecordChapter` on Hybrid MP4 and its refusal on MKV, whether `outputDuration` stops during a pause.
- The installer and the portable exe: SmartScreen, the WebView2 bootstrapper during install, the portable exe on a computer without WebView2.
- Global hotkeys against other programs, the tray at real DPI, DPAPI with a real OBS password, sounds through a real audio device.
- Performance on Windows: Host A and OBS latency, app CPU and memory while idle and during a burst.

## 2. Validate the import formats with real exports

Both SolidWorks importers report `validated: false` (DECISIONS item 4) because no real export was available.

- From SolidWorks 2025 and 2026: Tools > Customize > Keyboard, Category "All Commands", Copy List, pasted into Notepad and saved as `.txt`.
- From the same: Tools > Save/Restore Settings, a `.sldreg` with only "Keyboard shortcuts" selected.
- With those four files, replace the constructed fixtures in `crates/keystage-core/tests/fixtures/sw-shortcuts/` (the `.sldreg` ones are labeled hypothesis fixtures), confirm the delimiter, encoding and column handling, and either teach the `.sldreg` reader the keyboard section or keep refusing it. Then set `validated: true` for the formats that pass.

## 3. Code signing

The installer and portable exe are unsigned, so SmartScreen warns on first run until the files build reputation (Microsoft, SmartScreen reputation, https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation, read 2026-10-01).

- Signing needs an Authenticode certificate or a cloud signing service that the IDEA pathway or the school controls. The release workflow would sign `idea-keystage.exe` before bundling and the installer after; Tauri's bundler accepts a custom sign command on Windows.
- The secrets would live in GitHub Actions secrets for the release job only. Nothing else in the workflow changes.
- To decide: who owns the certificate, its cost, and whether a school-managed device policy makes the SmartScreen prompt moot.

## 4. Updates

There is no updater and no update check, by privacy rule 7 (no outbound network request).

- Tauri's updater plugin requires a signature that "cannot be disabled" (https://v2.tauri.app/plugin/updater/, read 2026-10-01), a key pair kept in release secrets, and a feed URL the app would poll. That poll is an outbound request.
- An option that keeps the rule: a "Check for a new version" button in settings that opens https://github.com/pina-hash/idea-keystage/releases/latest in the browser (the opener plugin already allows `https://` links). No background request.
- Another: an opt-in updater, off by default, that checks only when the user presses a button. It would need the signing key, a changed privacy rule and a line in the README.

## 5. Website integration (design only)

Nothing here is built and the app makes no network request. These are ways the IDEA website could use what Keystage already writes to disk.

- **Shortcut lists under each video.** The Markdown export ("Shortcuts used in this video") pasted into a lesson page or a YouTube description. Works today, by hand.
- **Session JSON as a lesson artifact.** A page on the IDEA site that reads a `keystage.session/1` file the teacher uploads (`docs/SESSION_FORMAT.md`) and shows a clickable shortcut timeline next to the video, with chapter links. The site would parse the file in the browser; nothing leaves the teacher's computer except the file they choose to upload.
- **Captions track.** The SRT export uploaded beside the video as a second subtitle track, so students can turn shortcut captions on or off.
- **A SolidWorks shortcut reference.** A page generated from `data/profiles/solidworks.json` at build time, listing every sourced shortcut with its Dassault source link and version, so students can study the same captions they see in videos.
- **Live classroom display.** An OBS-style source on the classroom projector already works on the teacher's own computer (`http://127.0.0.1:47820/overlay/keys`). Showing it on another computer would need the server to listen beyond `127.0.0.1`, which privacy rule 6 forbids; it would need its own design, an explicit switch, and a token.

## 6. Deferred features and refinements

- Cursor highlight that hides after the pointer has been still for a set time (Keystro parity item B2.5).
- An explicit ANSI or ISO keyboard choice (`keyboard.iso`), instead of switching to ISO the first time the 102nd key arrives.
- Keep the keyboard visual switch across restarts (a settings field; today it starts on at every launch).
- `ContextEvent::DisplaysChanged`, so a monitor added or removed is picked up at once rather than by the shell's 2 s poll.
- Time stamps on `ContextEvent::Password` and `Foreground`, so keys typed right after a relaxing change in the same pass can be shown instead of hidden for up to 16 ms.
- After a discarded password hold, read Caps Lock again (`InputReceiver::take_caps_lock_unsynced`) so later typed values never show inverted letter case.
- `save_settings` with a `base`, to merge on the app side and close the last few milliseconds in which a tray change can be overwritten by the settings window.
- `ExportWarning::MarkersBeforeVideo`, to count markers from an earlier recording that the chapter export leaves out.
- A step kind for "a modifier held while several keys are typed", so Alt symbol codes could be captioned (the documentation does not say numeric keypad, so the keys would still need a source).
- The settings preview stops its live transport while the window is hidden.
- `SO_EXCLUSIVEADDRUSE` on the server socket, so no other local program can bind port 47820 after Keystage.
- A WebView2 offline installer variant for computers that install without internet.
- A Playwright check that callouts never overlap the keyboard or mouse diagram at 1080p and 4K, and a Vitest project with browser conditions for Svelte component tests.
- `npm run format:check` in CI, now that the docs are formatted.
- Profiles for other applications, written from their own published documentation.
