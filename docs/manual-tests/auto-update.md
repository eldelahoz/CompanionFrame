# Auto-update (Windows)

Needs two published releases built with the updater (the first release with it, say 0.3.0, cannot update to itself). Below, OLD is installed and NEW is the published release after it.
Source: `src/stores/updater.ts`, `src-tauri/tauri.conf.json` (`plugins.updater`), `.github/workflows/release.yml`.

## Release side (after publishing NEW)

1. NEW's assets include `CompanionFrame_<NEW>_x64-setup.exe`, its `.sig` and `latest.json`.
2. Open `https://github.com/eldelahoz/CompanionFrame/releases/latest/download/latest.json`: `version` is NEW, `platforms.windows-x86_64` has a `url` and a non-empty `signature`.
3. While NEW is still a **draft** the same URL must not show NEW (drafts are ignored).

## App side

1. Install OLD with its setup.exe and start it. Settings > About: "Check for updates automatically" is on and shows "Current version: vOLD".
2. Wait about 10 s: a banner "Version NEW is available" appears bottom-left with Update now, Release notes and Later. Nothing else is blocked.
3. Release notes opens the release page of vNEW in the browser.
4. Later hides the banner. It does not come back after the next 6 h check (set the clock forward, or restart: it then shows again).
5. Settings > About > Check now: shows "Checking...", then the banner again (a manual check ignores Later).
6. Update now: the banner shows download progress, then the installer shows a small progress window (no questions, no UAC), the app closes and starts again. Settings > About shows vNEW. Settings and data are kept.
7. Check now on NEW: "You are up to date".

## Failure cases

- Turn the network off, Check now: the status shows "Update failed: ..." in red. With the switch on and the network off at start, nothing is shown.
- Turn the switch off and restart: no request to github.com after start (check with a proxy or firewall log).
- Build NEW with a different private key: Update now fails with a signature error in the banner and OLD keeps running.
- In `pnpm tauri dev` the pubkey is the placeholder: Check now shows an error, the automatic check is silent, the app works.
