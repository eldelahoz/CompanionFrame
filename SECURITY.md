# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub Security Advisories: open the **Security** tab of this repository and choose **Report a vulnerability**. Do not open a public issue for it.

Include the app version, your OS, what you did, what you expected and what happened, and a proof of concept if you have one. Remove tokens, cookies and account ids from anything you attach. This is a volunteer project: expect an acknowledgement within a few days and a fix or a plan after that. Credit is given if you want it.

## Supported versions

Only the latest release gets fixes.

## Scope

In scope:

- The desktop app (`src/`, `src-tauri/`) and its installer.
- The WebView-to-Rust boundary. The WebView is treated as untrusted input: commands validate payloads (`deny_unknown_fields`, range checks, hex ids safe for URL paths), and each overlay window can only use `core:event:default`.
- Handling of the warframe.market token, webhook URLs and the game's auth string.
- The opt-in game-memory mode and the warframe-api-helper download (pinned release, SHA-256 checked).
- Updates: the Tauri updater only installs a package whose minisign signature matches the public key in `tauri.conf.json` (it cannot be disabled). It fetches `https://github.com/eldelahoz/CompanionFrame/releases/latest/download/latest.json`, which skips draft releases, and sends no user data.
- Network code: requests go through `net.rs` / `wfm/client.rs` (HTTPS, timeouts, body caps, no private targets for user webhooks).

Out of scope:

- Vulnerabilities in Warframe, warframe.market, Windows or third-party services themselves.
- Using the app against the Warframe Terms of Service. The memory mode may violate them; see the disclaimer in the [README](README.md). That risk is documented and opt-in, not a vulnerability.
- Attacks that need local administrator rights or write access to your user profile.
- The fixed AES key and IV in `inventory/aleca.rs`: they belong to AlecaFrame's public cache format, not to a secret of this project.

## What is stored, and where

CompanionFrame has no server and no telemetry. Everything is local:

| Data | Where | Notes |
|---|---|---|
| warframe.market login token and user name | OS credential store (Windows Credential Manager, Secret Service on Linux), service `com.companionframe.app`, entry `wfm-session` | The password is zeroized after sign-in and never stored. If the credential store refuses, the login stays in memory only. |
| Discord/generic webhook URLs | OS credential store | Treated as secrets. HTTPS only. |
| Settings, notification channels, hotkeys | JSON files in the app data folder `com.companionframe.app` | No secrets. |
| Caches, price history, trade ledger, runs, inventory snapshot | `companionframe.db` (SQLite, with its `-wal` and `-shm` files) in the same folder | Contains your own game data. Not encrypted. |
| Database backups | `backups` folder next to the database | A copy of the database made before an upgrade that changes it; the 3 newest are kept. Same data, same lack of encryption: delete or protect them like the database. |
| Logs | `%LOCALAPPDATA%\com.companionframe.app\logs` (Windows), `~/.local/share/com.companionframe.app/logs` (Linux) | Request and response bodies are never logged; errors are stripped of URLs. Check a log before you share it. |
| OCR scan captures | `logs/scans` in the log folder, last 20 | Only if you enable "Save scan captures for troubleshooting"; off by default. Nothing is uploaded. |

The game's `EE.log` is only read. The app never writes to or injects into the game process.

## Game-memory mode

Off by default. Switching it on requires ticking an unchecked-by-default box under the disclaimer (Setup, or Settings > Inventory); the app refuses to save `memory` mode without that consent. The gate lives in `services/memory_gate.rs` and covers the native memory read, the helper download and run, and mission-reward tracking. In memory mode the game's auth string (`accountId` / `nonce`) is read from memory or the helper, used only to build the request to DE's own hosts, and never stored, logged or sent anywhere else. Two tests keep this honest: `src-tauri/tests/memory_reading_is_gated.rs` (the read API appears only in `platform/windows/process.rs`, behind the gate) and `src-tauri/tests/no_wfhelper_hosts.rs` (no source names the hosts of the original WFHelper backend).

> This project is not affiliated with or endorsed by Digital Extremes. Using tools that read game client memory may violate the Warframe Terms of Service and could result in action against your account. You use this software at your own risk.

## What the app does not do

- It sends nothing to a CompanionFrame server (there is none).
- It places no orders, auctions or +rep by itself: every warframe.market write is a click of yours. The one exception is a setting that is off by default (Settings > warframe.market & presence, "Close matched listings automatically"): when you turn it on, the app closes the listings a finished trade matches. Its description warns that warframe.market treats automation as a grey area.
- It does not modify the game.

More detail: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#security-boundaries).
