# Offline replay

Real screenshots go through the real scanners and the real overlay page, with no game and no network.

```
pnpm replay                       # all captures in $CF_SCREENSHOTS
CF_SCREENSHOTS=D:/shots pnpm replay
cargo run --manifest-path src-tauri/Cargo.toml --example replay -- D:/shots   # a directory argument works too
CF_REPLAY_LOCALE=en pnpm replay   # force the game language of every image
```

Needs `pnpm game-data` and `pnpm fetch-onnx` (models, ONNX runtime) and Microsoft Edge (`playwright-core` drives the installed Edge; no browser download). Windows only, like the scanners.

Captures are read from the directory argument, else `$CF_SCREENSHOTS`. One of the two is required: without it both steps exit with code 2 and say so. They should be 1920x1080 and stay outside the repository (they show player names and game art): never commit them. When the folder does not exist, both steps print a line and exit 0.

## What it does

1. `src-tauri/examples/replay.rs` (`cargo run --example replay -- [dir]`; `dir` overrides `$CF_SCREENSHOTS`), per image:
   - runs the app's detectors and logs each one: relic filter label (`relic_planner::era`), reward card count (`reward_scan::counter`), riven card locator (`riven_scan::locate_cards` / `locate_dimmed`). The app has no single screen classifier: EE.log says compare vs single and which screen is open. The harness takes the detected screen, except for the two captures in `SCREENS`, where it takes the user's label and logs the disagreement.
   - runs the pipeline the app runs for that screen: `overlay::riven::read_card` / `read_compare` + `riven_analyzer::analyze` (reward: `reward_scan::flow::scan_once` + `enrich`; relic: `era::detect`) with the game language (`fr`, `en` for the names in `EN`), the empty inventory, no good-roll sheet.
   - the market is a stub: six listings that carry exactly the card's stats, priced from the stat qualities. Reward prices are not filled.
   - writes `src-tauri/target/replay/<name>.json` (`{image, locale, screen, events: [{event, payload}]}`, each payload the exact view the backend emits: `riven-overlay`, `reward-overlay`, `relic-planner`) and `<name>.log` (detector output, OCR candidates, matches). The relic database goes to `relic-db.json` for the planner.
2. `scripts/replay/render.mjs` starts Vite, opens `overlay.html?kind=riven|reward` in a 1920x1080 Edge page on a mocked Tauri bridge (settings default, no inventory so the "No inventory" badges show, default placement), delivers the events, puts the screenshot behind as the page background and saves `<name>.overlay.png`.

Neither step touches app code paths beyond what the app already exposes; `riven::card_stats` and `riven::compare_verdict` were made `pub` for the stub analyzer.

## Limits

- The compare screen's CURRENT card falls back to the card remembered from before the roll (session state). Replay has none, so a dimmed card the locator misses is simply absent.
- No EE.log: planner and reward triggers, the 700 ms retries and the hide timers are not exercised (`scan_once` runs once).
- Windows OCR is used as the app uses it (game-language pack when installed, else en-US).
