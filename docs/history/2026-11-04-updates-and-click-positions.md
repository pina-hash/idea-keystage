---
title: 'IDEA Keystage 0.2.0: automatic updates from GitHub Releases verified against SHA256SUMS, click positions and tutorial capture in the Session Log (`main`, no migration)'
date: 2026-11-04
branches: [main]
migrations: []
subsystems:
  ['Updates', 'Input core', 'Session Log', 'App shell', 'Settings window', 'Docs', 'CI and release']
---

Mr. Pina asked on 2026-11-04 for IDEA Keystage to update itself ("It should just pull the automatic update every time") and for the Session Log to hold where each click happened, so a tutorial builder can line a screen recording up with known click points. A finding from his laptop the same day added tutorial capture: plain left clicks make no callout by default, so they never reached the log.

## What changed

- **Privacy rule 7** (ARCHITECTURE.md section 8, CLAUDE.md) now allows exactly the update check and download against this repository's GitHub releases over HTTPS, sending nothing about the user, besides the OBS socket. Rule 4 names the new Session Log contents. DECISIONS items 9 and 10 record why and on whose request.
- **`crates/keystage-update`**: semver comparison, the release choice (final releases only, exact asset names and download addresses, size limits), `SHA256SUMS.txt` reading, the `ureq` HTTPS client (rustls, `https_only`, no cookies), the schedule (20 s after start, every 6 hours, 30 minutes after a failure) and the deferral while a Session Log or an OBS recording runs, the installer switches and the marker the next run reads.
- **App shell**: the `keystage-update` worker, `Status.update`, the `update_check` command, the tray's update line and Check for updates, `ShellExecuteW` in the platform layer, `--updated` starting in the tray. The settings window's General page has an Updates card ("Check for updates automatically", on by default, a status line and Check now); the Privacy page and the README say what the check sends.
- **Session format 1.1**: mouse callouts carry `x`, `y` (physical pixels, virtual screen) and the shell adds `monitor`; the wire leaves them out, so the protocol fixtures did not change. "Tutorial capture: log every click" (Sessions page, off by default) adds `pointer`: presses, releases, double clicks, drags, wheel steps and a 30 per second cursor path, logged by the processor only while nothing is suppressed.
- Version 0.2.0 everywhere `tests/unit/versions.test.ts` checks, plus the new crate.

## Why it is built this way

The official Tauri updater cannot run without a minisign signing secret, which the release workflow does not have; the release already publishes `SHA256SUMS.txt`, so the app verifies against that and runs the same NSIS switches the official updater uses, read from the tauri-bundler 2.10.1 template rather than assumed. Installs never interrupt a recording. Click points stay off the wire because nothing but the Session Log needs them.

## What was measured

On the Linux container, 2026-11-04: `cargo test` (default members, including 23 new `keystage-update` tests with a local HTTP server for the download and checksum path), `npm run check` (0 errors, 0 warnings), `npm test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all --check`, and `cargo xwin check` and `cargo xwin clippy --all-targets -- -D warnings` for `idea-keystage` on `x86_64-pc-windows-msvc` (cargo-xwin 0.23.1). The app and platform tests run on `windows-latest` in CI.

## What is not verified

- The real update on Windows: the check from a school network, the passive install closing and relaunching the app, and "Updated to" on the next start. Nothing here runs Windows. The first proof is the first release after 0.2.0 arriving by itself.
- 0.1.0 has no updater: installing 0.2.0 over it is one manual step.
- Click positions on a multi-monitor desktop with mixed DPI scaling.
