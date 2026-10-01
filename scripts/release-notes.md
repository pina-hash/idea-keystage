IDEA Keystage {{VERSION}} for Windows 10 and Windows 11 (64-bit).

IDEA Keystage shows your keystrokes, a keyboard and your mouse while you record SolidWorks tutorials, either in its own on-screen overlay or as OBS Browser Sources. It works on this computer only: no account, no telemetry, no update checks, no internet connections. Its OBS sources are served to this computer only (127.0.0.1), and it talks to OBS only if you turn that on. Nothing is shown while a password field has focus.

## Downloads

| File                                         | What it is                                                                                                                                        |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `IDEA-Keystage_{{VERSION}}_x64-setup.exe`    | Installer (recommended). Installs for your Windows account only, so it needs no administrator rights. Adds a Start menu entry and an uninstaller. |
| `IDEA-Keystage_{{VERSION}}_x64-portable.exe` | Portable app. One file, nothing installed. Keep it in a folder you own and run it from there.                                                     |
| `SHA256SUMS.txt`                             | SHA-256 checksums of both files.                                                                                                                  |

## Install

1. Download `IDEA-Keystage_{{VERSION}}_x64-setup.exe`.
2. Run it. If Windows says "Windows protected your PC", read the SmartScreen note below.
3. Start IDEA Keystage from the Start menu. First-run setup asks which monitor to use, how the overlay should look, and whether you record with OBS sources, the on-screen overlay, or both.
4. IDEA Keystage then runs in the notification area. Open its settings from the tray icon.

To record with OBS, open the OBS page in the settings, copy a source address (for example `http://127.0.0.1:47820/overlay/keys`), and add it in OBS as a Browser Source with the width and height the settings page shows.

The default pause hotkey is Ctrl + Alt + Shift + P. Pausing stops all input capture until you resume.

To uninstall, open Windows Settings, then Apps, then Installed apps.

## SmartScreen

These files are not code signed, so Microsoft Defender SmartScreen may show "Windows protected your PC" the first time you run one. To continue, click More info, then Run anyway.

Before you do, you can check that the file is the one published here. In PowerShell, in your Downloads folder:

```powershell
Get-FileHash .\IDEA-Keystage_{{VERSION}}_x64-setup.exe -Algorithm SHA256
```

The hash must match the line for that file in `SHA256SUMS.txt` (upper or lower case does not matter).

## WebView2

IDEA Keystage draws its windows with Microsoft Edge WebView2.

- The installer installs the WebView2 Runtime if it is missing. That step needs an internet connection during setup.
- The portable app cannot install it. Windows 11 includes it and most Windows 10 computers already have it. If the portable app does not open, install the Evergreen WebView2 Runtime from Microsoft (https://developer.microsoft.com/en-us/microsoft-edge/webview2/) or use the installer.
