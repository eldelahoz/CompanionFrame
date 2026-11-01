# F6-T6 manual test: reward scanner on the live game

This runs the capture and OCR pipeline on its own against the real reward screen. The hotkey and overlay flow is in f6-t9-t10-reward-overlay.md.

1. Start Warframe in borderless fullscreen, 1920x1080 or your usual resolution, UI scale at your usual setting.
2. From `src-tauri/`: `cargo run --release --example ocr_fixtures -- --scan`.
3. Within 5 s, run a mission to the reward screen (or open a finished one) so the 1-4 reward cards are on screen with Warframe in front.
4. Read the output: `N card(s) in X ms` and one `slot i: [...]` line per card.
5. Check:
   - **N matches the cards on screen** for 1, 2, 3 and 4 players (try a solo run and a squad).
   - **Titles read right**, including a name that wraps to two lines (e.g. "... Chassis / Blueprint"); the joined text must contain the full name.
   - **Time per frame < 400 ms** in the release build.
   - Repeat with a non-default UI scale (Settings > Interface) and a 16:10 or ultrawide window. If N is 0, note the scale and resolution.
6. Capture caveat: if the scan reports `no reward cards found` while cards are visible, run `-- --screen` and check the OCR text is not empty (a black capture would be an F6-T3 issue, not this one).
