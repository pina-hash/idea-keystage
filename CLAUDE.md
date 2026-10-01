# CLAUDE.md

Instructions for any agent working in this repository. Read `docs/ARCHITECTURE.md` (the contract) before changing code, and `docs/DECISIONS.md` before changing a decision.

IDEA Keystage is a Windows 10 and 11 (x64) keystroke, keyboard and mouse visualizer for recording SolidWorks tutorials, with a native overlay window (Host A) and OBS Browser Sources served from `127.0.0.1:47820` (Host B). It must be beautiful, professional, private and fast.

## Working mode

- **Solo mode is the default.** Every commit lands on `main`. No feature branches, no pull requests. Push with `git push origin HEAD:main`.
- Commit only when asked. Keep `Cargo.lock` and `package-lock.json` committed and in step.
- A release is a `v<version>` tag on `main`. The tag must equal `version` in `src-tauri/tauri.conf.json` (also `package.json` and `Cargo.toml`; `tests/unit/versions.test.ts` checks all of them).

## Stack

- Rust 1.97.0 (pinned in CI; `rust-version` 1.90), Tauri 2.12.1 with tauri-build 2.7.1 and the plugins single-instance, global-shortcut, autostart, dialog, clipboard-manager and opener; `windows` 0.62.2; axum 0.8.9 and tokio 1.53.1 for the local server; rodio 0.22.2 for sound.
- Svelte 5.57.1 and TypeScript 6.0.3, Vite 8.3.1 (build target `chrome103`), Vitest 5.0.3 with jsdom, svelte-check 4.7.6, Prettier 3.9.9, Playwright 1.56.1 (scripts only).
- Node 22.12 or later. Every dependency is pinned to an exact version: Rust in `[workspace.dependencies]` of `Cargo.toml` (`=x.y.z`), npm in `package.json`. Add a dependency only when nothing pinned will do, with an exact version from https://index.crates.io or https://registry.npmjs.org, and say why in the commit.

## Layout and file surfaces

| Path                                                                                                                                                     | Surface                    | What it is                                                                                                                                   |
| -------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/keystage-core/`                                                                                                                                  | input core                 | Pure Rust (no IO, threads, async, OS calls or logging): events, processor, profiles, imports, settings, protocol, IPC types, sound synthesis |
| `crates/keystage-session/`                                                                                                                               | server, OBS, exports       | Pure Rust: Session Log, recording clock, SRT, Markdown, JSON and chapter exports                                                             |
| `crates/keystage-server/`                                                                                                                                | server, OBS, exports       | axum server on `127.0.0.1`, WebSocket hub, routes, `demo` example binary                                                                     |
| `crates/keystage-obs/`                                                                                                                                   | server, OBS, exports       | obs-websocket 5 client                                                                                                                       |
| `src-tauri/src/platform/**`                                                                                                                              | Windows layer              | Hooks, watcher, UI Automation password check, legends, monitors, window styles, DPAPI; `stub.rs` off Windows                                 |
| `src-tauri/src/app/**`, `lib.rs`, `main.rs`, `tauri.conf.json`, `capabilities/`, `permissions/`, `icons/`, `build.rs`                                    | app shell                  | Pipeline, Host A, tray, hotkeys, IPC commands, settings store, sessions, sound playback                                                      |
| `src/overlay/**`, `src/lib/renderer/` (except below), `src/lib/protocol.ts`, `src/lib/themes/**`, `src/lib/styles/**`, `src/lib/demo/**`, `overlay.html` | renderer and themes        | The one renderer for Host A, OBS and the settings preview                                                                                    |
| `src/lib/renderer/keyboard/**`, `MouseDiagram.svelte`, `CursorFx.svelte`                                                                                 | keyboard and mouse visuals |                                                                                                                                              |
| `src/settings/**`, `settings.html`                                                                                                                       | settings window            |                                                                                                                                              |
| `data/profiles/*.json`                                                                                                                                   | sourced data               | SolidWorks profile; never edited from memory                                                                                                 |
| `scripts/**`, `.github/**`, `tests/unit/**`, `tests/e2e/**`                                                                                              | CI and release             | Workflows, screenshot and latency harnesses, cross-language tests                                                                            |
| `docs/**`, `README.md`, `CLAUDE.md`, `AGENTS.md`                                                                                                         | docs                       |                                                                                                                                              |

When several agents work at once, each writes only the files of its own surface and asks for changes elsewhere in its notes. Keep `docs/ARCHITECTURE.md` and the code in agreement: when one changes, change the other in the same commit.

## Commands

```sh
npm ci
npm run check            # svelte-check, zero errors and zero warnings
npm test                 # Vitest: src/**/*.test.ts and tests/unit/**
npx vitest run src/lib/renderer   # one folder
npm run build            # dist/ (overlay.html and settings.html)
npm run format:check     # Prettier, repo config

cargo test               # default members: core, session, server, obs (never builds Tauri)
cargo test -p keystage-core
cargo test -p idea-keystage                     # the app and platform tests
cargo clippy --all-targets -- -D warnings
cargo clippy -p idea-keystage --all-targets -- -D warnings
cargo fmt --all --check

# Windows checks from Linux (cargo-xwin 0.23.1, target x86_64-pc-windows-msvc)
cargo xwin check -p idea-keystage --target x86_64-pc-windows-msvc
cargo xwin clippy -p idea-keystage --target x86_64-pc-windows-msvc --all-targets -- -D warnings
npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis   # local cross build, needs makensis

# Windows
npm run tauri dev
npm run tauri build

# Harnesses (Linux, Playwright Chromium)
cargo run -p keystage-server --example demo -- --port 47820 --dist dist [--script solidworks|general|burst|idle]
npm run shots            # scripts/screenshots.mjs, output in .work/shots/final
npm run perf             # scripts/perf.mjs, output in .work/perf
UPDATE_GOLDEN=1 cargo test -p keystage-session --test golden   # regenerate export goldens, then read every change
```

Formatting: Rust uses the repo `rustfmt.toml` (`cargo fmt -p <crate>`, or `rustfmt --edition 2021 <files>`); the front end and Markdown use the repo Prettier config (`npx prettier --write <files>`). Format only the files you changed, never the whole tree. Run the tests for what you touched; CI runs everything (`.github/workflows/ci.yml`: Linux front end and core crates, Windows app build, tests and clippy).

## Privacy rules (non-negotiable)

From `docs/ARCHITECTURE.md` section 8. Every change is reviewed against them.

1. The hook callbacks copy a fixed-size record (stamped with the password hold word, one atomic load) into a preallocated lock-free queue, unpark the pipeline thread, call `CallNextHookEx` and return. No allocation, no logging, no locks, no other system calls.
2. Pausing uninstalls both hooks. Resuming reinstalls them and resyncs modifiers.
3. A password field (UI Automation `IsPassword`, or an Edit control with `ES_PASSWORD`), an excluded app, a locked session or an unidentified foreground suppresses everything: no display, no keyboard light, no log entry, no sound. Keys typed between a focus change and its classification are held and dropped unless the new focus is "not a password". Context changes that hide apply before queued input; changes that show more apply after it. Without UI Automation, typing is masked in every profile. Fail closed whenever in doubt.
4. Nothing typed is written to disk except a Session Log the user started (hotkey, tray, settings, or an OBS recording while the user has turned on the OBS connection, whose "Session log with each recording" switch is on by default), and a Session Log stores only the `callout` messages that were displayed plus chapter markers.
5. `tracing` output never contains key names, text, callouts, captions, settings values, app names or import contents. Lifecycle only; settings issues by field name only.
6. The server binds `127.0.0.1` only and enforces the WebSocket Host and Origin rules (section 6). No CORS headers.
7. The app makes no outbound network request. The only socket it opens is to the obs-websocket host the user configured, while OBS is turned on. No telemetry, no update check.
8. The OBS password is stored only as a DPAPI (CurrentUser) blob, never in plain text, never exported.

## SolidWorks data is sourced, never recalled

- Never write a SolidWorks shortcut, command name or mouse caption from memory, in code, tests, demos, docs or examples. Use only `data/profiles/solidworks.json` (every entry cites a Dassault help page, version, date and verbatim quote) or the user's own import.
- Tests and demos read captions from that file; fixtures use synthetic names (`Fixture Command One`, `Fixture undo`).
- Changing the data file needs a source: the page URL, version, date read and the verbatim quote. Gaps are reported in its `gaps` array and in `docs/DECISIONS.md` item 3, not filled.
- No SolidWorks or Dassault logos or artwork. "SolidWorks" appears only as plain text naming the profile. No Keystro names, assets, themes or sounds.

## Writing and design rules

- American spelling. No em dashes anywhere (UI copy, code comments, docs, commit messages). Sentence case labels. Short, plain sentences. No placeholder text and no TODO stubs in shipped paths.
- Fonts: only Rajdhani, Share Tech Mono and Orbitron, bundled from `@fontsource`. No CDN, no other fonts.
- No grid backgrounds anywhere. Color is never the only signal (a pressed key also changes shape). Contrast is measured (4.5:1 for text). 44 px minimum targets in the settings window.
- Everything is vector or CSS; no raster art in the overlay.
- The overlay must run in Chromium 103 (OBS 30): no `:has()`, `@container`, `color-mix()`, `oklch()`, CSS nesting, `popover`, `@starting-style`, the `translate` property, dynamic viewport units, `toSorted`, `Promise.withResolvers` without a finished-looking fallback.
- Every piece of logic gets a test.

## Where things are written down

`docs/ARCHITECTURE.md` (contract), `docs/DECISIONS.md` (research and decisions with sources), `docs/SESSION_FORMAT.md`, `docs/PARITY.md`, `docs/SOURCES.md`, `docs/ROADMAP.md`, `docs/history/` (one entry per bundle), `docs/prompt-ledger/entries/` (one entry per prompt).
