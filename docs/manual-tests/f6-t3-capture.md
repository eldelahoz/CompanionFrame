# F6-T3 manual test: game window capture (needs Warframe)

## Design
Capture is a **GDI BitBlt of the monitor's screen DC** (the final image after DWM/MPO composition), cropped to the client rect of the top-level window titled `Warframe`
(`FindWindowW` + `GetClientRect` + `ClientToScreen`; skipped if minimised or smaller than 320x240). If the window is not found it captures the whole monitor
(`platform/windows/{capture,game_window}.rs`). Reading the *screen* DC (not the window DC) is what avoids the black frame on DX11/DX12 flip-model windows.
There is no capture fallback after a GDI failure, because desktop capture APIs can return stale MPO content. A `looks_black` check logs a `warn` as a diagnostic.
WGC/PrintWindow only if step 2 or 3 below fails.

## Steps
1. `pnpm tauri dev`, open devtools in the main window.
2. Game in **borderless**, in a match/orbiter (not a loading screen). In the console:
   `await window.__TAURI_INTERNALS__.invoke('capture_game_window_info')`
   Expect `found:true`, `source:"window"`, `black:false`, `width/height` = game resolution, `dpi` = 96/120/144 for 100/125/150 %.
3. Repeat in **windowed** mode: width/height is the client area only (no title bar); `source:"window"`, `black:false`.
4. Drag the game window to the **second monitor** (and back), invoke again: `x,y` follow it, `black:false`. Try a monitor at a different scale.
5. Minimise the game: `found:false`, `source:"screen"` (primary monitor captured), no error.
6. Close the game: same as 5.
7. **Black-frame check** (the open risk): if any of 2-4 gives `black:true` while the game shows an image, BitBlt does not work
   for that mode: copy the `capture: frame is black` line from `%APPDATA%\...\logs\companionframe.log`, and note mode
   (borderless/windowed/exclusive fullscreen), API (DX11/12), GPU. Exclusive fullscreen is expected to be unsupported (out of scope).
   A loading screen can legitimately be black: retry in-mission before reporting.
8. Save a frame to eyeball it: `cargo run --example ocr_fixtures -- --screen` OCRs the primary screen after 5 s (game in front).
   (Not a window-crop test; use step 2-4 for geometry.)

## Without the game
`cargo run --example capture_leak` (from `src-tauri/`): 20 captures, GDI object count must not grow (printed `before -> after`).
