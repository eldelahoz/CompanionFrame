# F6-T16 – Arbitration live overlay: manual test

Automated: `cargo test -p companionframe arbi` replays `src-tauri/tests/fixtures/eelog/arbi/*` through the live tracker; `src/overlays/arbi/ArbiOverlay.test.tsx` covers the screen.

## With the game (borderless, Windows 11)
1. `pnpm tauri dev`, start Warframe, enter an Arbitration (defense or interception; host, so the log has AI lines).
2. Once the mission loads the counter appears top-right without taking focus; the game keeps keyboard focus.
3. While fighting: time runs, kills and Drones/min grow, "Time at 15+ Enemies" moves. It updates about twice a second.
4. Finish or abort the mission: the counter disappears at the end marker (EOM / abort), not on the next mission.
5. Hide: in Settings > Overlays & hotkeys untick "Arbitration overlay" mid-run; the window closes at once and stays closed for the next run. Tick it again and the next run shows it.
6. Look: readable over the game scene, matches the theme/opacity of the other overlays; with the interaction mode on, drag it (position persists).

## Known gaps
- Position is the shell drag offset, saved in the Rust store (`offset:arbi`).
- Client-side runs (not host) have no AI lines, so drones/kills stay at 0.
- Elapsed time can drop once early in a run when the start anchor moves from mission load to first combat.
