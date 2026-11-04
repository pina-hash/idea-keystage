---
title: 'IDEA Keystage 0.3.0: the Claude connector (a stdio MCP relay to an authenticated control API), human-like mouse and keyboard with an abort on any human input, clean-take checks, marker steps (`main`, no migration)'
date: 2026-11-04
branches: [main]
migrations: []
subsystems:
  [
    'Connector',
    'Server and OBS',
    'App shell',
    'Windows layer',
    'Renderer',
    'Settings window',
    'Session Log',
    'CI and release',
    'Docs',
  ]
---

Mr. Pina asked on 2026-11-04 for IDEA Keystage to be something Claude can drive "whenever you need to make a tutorial for something", for any app (SOLIDWORKS, electrical and software tutorials for FRC), and then for the real cursor to move like a person's. Findings from his laptop the same day added clean takes (the Claude window floats over SOLIDWORKS), OBS control without hotkeys, and waiting for slow dialogs.

## What changed

- **Control API** on the existing `127.0.0.1` server (`crates/keystage-server/src/control.rs`): `GET /api/control/tools`, `POST /api/control/call`, a per-install token in `control.json`, constant-time compare, and no requests with browser headers. The overlay routes are unchanged.
- **`idea-keystage-mcp`** (`crates/keystage-mcp`): a stdio MCP server that relays to the app, so tools arrive with app updates; `IDEA-Keystage_<version>_connector.mcpb` (MCPB manifest 0.3, binary server) and the bare exe are release assets.
- **Tools** (`crates/keystage-automation/src/tools.rs`): status, Session Logs with marker steps, OBS start and stop through the OBS link, the overlay, profiles, pause, recordings by chunk, updates, the foreground window, occluders, and the input tools: move, click, drag, scroll, chord, type, wait_idle and `perform`.
- **Motion engine** (`keystage-automation` plus `src-tauri/src/app/automation.rs` and `platform/windows/automation.rs`): curved minimum-jerk paths timed by Fitts's law with overshoot, Catmull-Rom drags, human typing cadence, `SendInput` with virtual-desktop absolute coordinates, an abort watch on any non-injected input or Escape, and release of anything held.
- **Settings**: "Allow the IDEA Keystage connector" (on) and "Allow Claude to move the mouse and type" (off, also in the tray). The wire state gained `automated`; Host A and the preview show "Automated", OBS sources never do.
- **Session format 1.2**: markers keep an optional `step`. **Privacy**: rule 6 names the control API, rule 9 is new.
- Version 0.3.0.

## What was measured

On the Linux container, 2026-11-04: `cargo test` over every default member (including the new automation, MCP relay, control API and OBS request tests), `npm run check`, `npm test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all --check`, `npm run format:check`, and `cargo xwin clippy -p idea-keystage --all-targets -- -D warnings` for `x86_64-pc-windows-msvc`. Windows app tests run in CI.

## What is not verified

- Everything on Windows: Claude installing the bundle, a call reaching the app, the cursor and keys through `SendInput` (and at mixed DPI), the abort on a real person's input, `occluders` with the Claude window, `wait_idle`, OBS start and stop from the connector. This container has no Windows.
- The official `mcpb` CLI was not run on the bundle (running outside packages was not allowed in this session).
- No release was published: pushing the `v0.2.0` tag was refused by the session's git proxy (HTTP 403), and creating it through the API was refused too. A person creates the tag; the session report gives the steps.
