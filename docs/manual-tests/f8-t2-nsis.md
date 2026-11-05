# F8-T2 – NSIS installer (Windows)

Config: `src-tauri/tauri.windows.conf.json` (Windows-only override; NSIS only, per-user, en/de/zh/fr with language selector, embedded WebView2 bootstrapper).
Unsigned, so SmartScreen warns. Build: `pnpm tauri build` -> `src-tauri/target/release/bundle/nsis/CompanionFrame_<version>_x64-setup.exe`.
The Release workflow (`.github/workflows/release.yml`) builds the same file and attaches it to the draft release; no renaming is needed.

## Measured (local build, 0.1.0)

| Item | Size |
|---|---|
| Setup.exe | 35.76 MiB (budget 70 MB) |
| Installed on disk | 129.3 MB |
| `companionframe.exe` | 19.9 MB |
| `game-data/` | 71.9 MB |
| `onnxruntime/onnxruntime.dll` | 17.3 MB |
| `riven-ocr/` (first build: YOLO 12.3 + Paddle rec 10.7 + dict; the detector is PP-OCRv5 now) | 21.9 MB |
| `uninstall.exe` | 0.1 MB |

Local smoke on the dev machine: `Setup.exe /S /D=<tmp>` installs without UAC, registers HKCU uninstall entry (DisplayVersion 0.1.0) and a Start Menu shortcut,
`companionframe.exe` starts and shows a window titled "CompanionFrame", `uninstall.exe /S` removes the install dir and leaves `%APPDATA%\com.companionframe.app`.

## Clean-VM check (Windows Sandbox or VM, no Rust, no VC++)

Run once with WebView2 present and once on Windows 10 without it (needs network the first time unless the bootstrapper is embedded: it is).

1. Double-click `CompanionFrame_<version>_x64-setup.exe`. No UAC prompt. SmartScreen "More info > Run anyway" is expected (unsigned).
2. The language selector lists English, Deutsch, 简体中文, Français. For each: the wizard texts are in that language.
3. Finish with "Run CompanionFrame" ticked: the app opens to the main window.
4. Start Menu > CompanionFrame and the desktop shortcut both launch the app.
5. Settings > Apps lists CompanionFrame with the right version.
6. Install again over the existing install (silent: `/S`): it upgrades, no duplicate entry.
7. Create some data (e.g. change a setting), then uninstall via Settings > Apps, leaving "Delete application data" unticked:
   install dir and shortcuts are gone, `%APPDATA%\com.companionframe.app` is kept; reinstall keeps the settings.
8. Reinstall, uninstall with the box ticked: the app data dir is removed.
