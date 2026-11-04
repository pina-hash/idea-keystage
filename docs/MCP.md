# IDEA Keystage connector for Claude

The connector lets Claude, in the Claude desktop app on the recording laptop, drive IDEA Keystage to make a tutorial for any Windows app (SolidWorks, Bambu Studio, VS Code, FRC Driver Station, REV Hardware Client, a browser): start a Session Log, start and stop the OBS recording, show or hide the overlay, check that nothing covers the screen, and, when you allow it, move the real mouse and type like a person. Everything stays on this computer. How it works is in `docs/ARCHITECTURE.md` section 14.

## Install once

1. **IDEA Keystage 0.3.0 or later.** If the laptop has 0.1.0, download `IDEA-Keystage_0.3.0_x64-setup.exe` from https://github.com/pina-hash/idea-keystage/releases/latest and run it (0.1.0 cannot update itself; every version from 0.2.0 on updates by itself). Start IDEA Keystage from the Start menu.
2. **The connector.** From the same releases page, download `IDEA-Keystage_0.3.0_connector.mcpb`. Double-click it. The Claude desktop app opens it; click **Install**. (Or open Claude, Settings, Extensions, and drag the file into the window.)
3. **OBS** (for recordings). In OBS: Tools, WebSocket Server Settings, turn on **Enable WebSocket server**, and note the server password shown there. In IDEA Keystage settings, on the OBS page, turn on **Connect to OBS** and enter that password.
4. **Mouse and keyboard** (only when you want Claude to run a take). In IDEA Keystage settings, on the General page, turn on **Allow Claude to move the mouse and type**. The tray menu has the same switch. It is off until you turn it on.

Then, in a Claude chat on the laptop, ask for the tutorial. Claude calls `status` first and tells you if something above is missing.

## While Claude records

- Touching the mouse or the keyboard, or pressing **Esc**, stops an automated take at once. Nothing is left pressed.
- The on-screen overlay shows an **Automated** pill while Claude moves the mouse or types. OBS sources never show it. If you record the whole screen with the on-screen overlay, the pill is in the video too; turn on **Keep out of screen capture** on the Display page to keep it out.
- Claude never types into a password field, an app on the Privacy page's excluded list, or anything while the computer is locked or IDEA Keystage is paused.
- A take refuses to start while another window (the Claude window included) covers the screen being recorded. Minimize that window and ask again. IDEA Keystage never moves or hides another app's window.

## If the connector cannot reach IDEA Keystage

Claude says "IDEA Keystage is not running". Start IDEA Keystage from the Start menu; the tools appear within 5 seconds. On the General page, **Allow the IDEA Keystage connector** must be on (it is by default).

## Without the bundle (fallback)

If double-clicking the `.mcpb` file does not work:

1. Download `IDEA-Keystage_0.3.0_x64-connector.exe` from the releases page and save it as `C:\Users\<you>\AppData\Local\IDEA Keystage\idea-keystage-mcp.exe` (the folder IDEA Keystage is installed in).
2. In the Claude desktop app: Settings, Developer, **Edit Config**. This opens `%APPDATA%\Claude\claude_desktop_config.json`. Put this in it (keep any servers already there inside `mcpServers`), with your Windows user name:

```json
{
	"mcpServers": {
		"idea-keystage": {
			"command": "C:\\Users\\<you>\\AppData\\Local\\IDEA Keystage\\idea-keystage-mcp.exe"
		}
	}
}
```

3. Quit Claude completely (tray icon, Quit) and start it again. The path must be absolute. If the tools do not appear, look at the `mcp*.log` files in `%APPDATA%\Claude\logs`.

The connector reads `%LOCALAPPDATA%\com.ideabosco.keystage\control.json`, which IDEA Keystage writes at every start (its port and this computer's connector key). Nothing else is configured.

Sources (read 2026-11-04): the MCPB manifest specification (https://github.com/modelcontextprotocol/mcpb, `MANIFEST.md`, manifest version 0.3), Anthropic's desktop extensions post (https://www.anthropic.com/engineering/desktop-extensions: "Double-click to open with Claude Desktop", "Click Install"), and the MCP guide to local servers (https://modelcontextprotocol.io/docs/develop/connect-local-servers: the Windows config path `%APPDATA%\Claude\claude_desktop_config.json`, Settings, Developer, Edit Config, absolute paths, logs in `%APPDATA%\Claude\logs`).
