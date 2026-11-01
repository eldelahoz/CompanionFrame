# F6-T1 overlay manager: manual test with Warframe

Setup: `pnpm tauri dev`, Warframe in borderless. Four hidden windows exist (`overlay-reward|riven|arbi|trade`); F8 toggles the reward one (default `overlay` hotkey, see `f6-t2-hotkeys.md`).
Open the webview devtools of the main window for `invoke`:
`const {invoke}=window.__TAURI_INTERNALS__` then `invoke('overlay_show',{kind:'trade'})`.

1. **Startup**: no taskbar button / Alt-Tab entry for any overlay; no overlay visible. Check `companionframe.log` has no "overlay windows failed".
2. **Basic behaviour** with F8 (reward), game focused: the hotkey works without alt-tab; no focus steal (the game does not minimise or flicker and keys reach it right after F8); click-through (click and drag through the overlay, the game receives it); stays above the game (toggle off/on, open map/menus, alt-tab away and back); re-show not black.
   - If a re-show is a black box: set `KEEP_MAPPED = true` in `services/overlay/host.rs` and retest (page must listen to `overlay-content-visible`; T5 adds that, so until then only the black-box question matters).
3. **All four**: `overlay_show` each kind in turn. Each appears over the game, game keeps keyboard focus (keep moving/typing).
4. **Second monitor**: move the game window to monitor 2, wait ~3 s (pid cache + 250 ms tick), show an overlay: it appears on monitor 2. Move the game back; it follows within ~3 s. Mixed DPI scaling on the two monitors: overlay must still cover the whole monitor.
5. **Focus loss**: alt-tab to another app: shown overlays hide within ~0.5 s; alt-tab back: they return. Click the CompanionFrame main window: overlays stay.
6. **Topmost**: with an overlay shown, open map/menus and alt-tab; it stays above. Click the taskbar; after ~0.5 s the overlay is back on top once the game is foreground.
7. **Clickable without focus (pending item 5)**: show `trade`, then
   `invoke('overlay_set_interactive',{kind:'trade',interactive:true})`.
   Click anywhere on the overlay (needs visible content, e.g. a button from T5/T17, or watch the cursor change). Expected: the click reaches the overlay, the game does NOT lose focus (keys still go to it, no title-bar greying). Then `interactive:false`: clicks pass through again.
8. **Regions**: `invoke('overlay_set_regions',{kind:'trade',regions:[{x:100,y:100,w:300,h:80}]})` (physical px, screen coords). Cursor over that rect takes clicks, everywhere else clicks reach the game. Clearing with `regions:[]` restores full click-through.
9. **Focus fallback**: if step 7/8 ever steals focus, the manager returns it with `SetForegroundWindow(game)` within ~250 ms. Note whether the game flickers or loses keystrokes in that window; report it.
10. **CPU**: with regions set, the process should stay near 0 % (50 ms cursor sampling).

**Known limit**: exclusive fullscreen is not supported (borderless only).
