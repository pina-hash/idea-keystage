# 0003 The IDEA Keystage connector for Claude and human-like input (0.3.0)

- Issued: 2026-11-04
- By: Mr. Pina's IDEA & FRC chat, relayed to the same Claude Code session as 0002
- Owns: `crates/keystage-automation/**`, `crates/keystage-mcp/**`, `mcpb/**`, `crates/keystage-server/src/control.rs`, the OBS client's request path, `src-tauri/src/app/{control,automation}.rs`, `src-tauri/src/platform/windows/automation.rs`, the shell, tray, settings and renderer pieces that show the connector, `scripts/pack-mcpb.mjs`, `scripts/lib/zip.mjs`, `.github/workflows/release.yml`, the version files, and the docs that describe them
- Migration permitted: no. Claims: none
- Status: pushed
- Branch: main
- Notes: Mr. Pina asked for "a connector to be able to connect with it and
  do a lot of things, like as many things as you can manage making
  automatic ... whenever you need to make a tutorial for something", then
  for the real cursor to "move like it would with a human user", then
  (from the laptop) for clean takes with the Claude window in front, OBS
  control without hotkeys, and waiting for slow dialogs.

  Built: the control API (token, no browser headers), the
  `idea-keystage-mcp` relay and its `.mcpb` bundle, 27 tools, the motion
  engine with the abort watch, occluders and dirty spans, `wait_idle`,
  `perform`, the "Allow Claude to move the mouse and type" switch (off by
  default), the "Automated" pill on Host A, session format 1.2 (marker
  steps), and privacy rule 9. Version 0.3.0. Tags could not be pushed from
  this session (the git proxy answered 403), so the 0.2.0 and 0.3.0
  releases wait for a person to create the tag.
