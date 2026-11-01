# F6-T5 – React overlay skeleton: manual test

Needs `pnpm tauri dev` on Windows; the game is only needed for step 5.

1. Start the app. The four overlay windows (`overlay-reward|riven|arbi|trade`) load `overlay.html?kind=<kind>` hidden.
   Show one (F8 or the T1 `overlay_show` command): a panel with the kind name appears, in the theme (dark/light) and language of Settings.
2. Open devtools on an overlay window and run `window.__TAURI_INTERNALS__` to confirm Tauri is present; no console errors.
3. From the main window devtools emit `overlay-theme-vars` with `{"--overlay-opacity":"50%"}`: the panel fades to 50 %.
   `{"--overlay-opacity":"10%"}` (below 30 %) is ignored and falls back to 100 %.
4. Emit `overlay-interaction-mode` with `{"interactive":true}`: left-drag the panel (or right-drag anywhere) moves it; the cursor becomes `grabbing`.
   Close and reopen the overlay: the offset is kept (webview localStorage). With `false`, dragging does nothing.
5. **[JUEGO]** With Warframe in borderless, repeat 1 and 4: no focus loss, click-through when not interactive.

Not covered yet: the real screens (T10, T15, T16, T17), the event names above are provisional until T9/T18 emit them.
