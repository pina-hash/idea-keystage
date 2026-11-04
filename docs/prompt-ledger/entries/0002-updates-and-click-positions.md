# 0002 Automatic updates and click positions in the Session Log (0.2.0)

- Issued: 2026-11-04
- By: Mr. Pina's IDEA & FRC chat, relayed to one Claude Code session
- Owns: `crates/keystage-update/**`, `crates/keystage-core/src/{pointer,event,processor,settings,ipc}.rs`, `crates/keystage-session/**`, `src-tauri/src/app/{updater,shell,pipeline,sessions,tray,actions,commands,state}.rs`, `src-tauri/src/platform/{mod,stub}.rs`, `src-tauri/src/platform/windows/mod.rs`, the settings window's General, Sessions and Privacy pages, the version files, and the docs that describe them
- Migration permitted: no. Claims: none
- Status: pushed
- Branch: main
- Notes: Mr. Pina asked for two things on 2026-11-04: "include an update
  for idea key stage to automatically update, so that doesn't have to be
  redownloaded manually every time. It should just pull the automatic update
  every time", which amends privacy rule 7, and the screen position of every
  click in the Session Log for automatically produced SolidWorks tutorials.
  A finding from his laptop the same day added "Tutorial capture: log every
  click", because plain left clicks make no callout by default.

  Built: the `keystage-update` crate and its worker (GitHub release check,
  SHA256SUMS verification, deferral while recording, passive NSIS install
  with `/P /UPDATE /R /ARGS --updated`), the settings switch, Check now and
  the tray items; session format 1.1 (callout `x`, `y`, `monitor`; `pointer`
  tutorial capture). Released as v0.2.0. The real update on Windows is
  verified only by the first install of a later version.
