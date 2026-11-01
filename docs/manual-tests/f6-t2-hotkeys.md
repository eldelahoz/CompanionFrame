# F6-T2 – Hotkeys with the game focused (manual test)

Needs Warframe in **borderless** and the app running with `pnpm tauri dev` (or installed). Default bindings:
`reward_scan` = F8, `overlay_move` = F7, `riven_rescan` = F6 (`settings.json` fields `reward_scan_hotkey`,
`overlay_move_hotkey`, `riven_rescan_hotkey`).

Each binding emits the `hotkey` event with its id. To watch it, run in the devtools console of the main window:
`window.__TAURI__.event.listen('hotkey', e => console.log(e.payload))` (needs `withGlobalTauri`), or use
`window.__TAURI_INTERNALS__.invoke` for commands when it is off.

## With the game
1. Start the app, then Warframe in borderless. The log (`app_log_dir`) must say `low-level keyboard hook installed`.
2. With the game in front press **F8** on the reward screen: the reward scan runs and the overlay shows. **The game must not react to F8** (the key is swallowed).
   Hold F8 for 2 s: one trigger only, no flicker (auto-repeat is ignored).
3. Press **F7** and **F6** with the game in front: two distinct `hotkey` events (`overlay_move`, `riven_rescan`), and the game does not receive them.
4. Alt-Tab to another app (Notepad, a browser) and press F8. It must **not** trigger the overlay, and the key **must reach** that app
   (or bind `Ctrl+Shift+K` and type it in Notepad).
5. Hot change: editing `settings.json` is not enough (it only applies when saved from the app). From the devtools:
   `s = await invoke('get_settings'); s.reward_scan_hotkey = 'Ctrl+Shift+K'; await invoke('set_settings', {settings: s})`.
   With the game in front, `Ctrl+Shift+K` now triggers the scan and F8 no longer does. No restart needed.
6. Rejections (each must return an error and change nothing): `reward_scan_hotkey = 'F7'` (duplicates the move key), `'Alt+F4'`, `'A'`, `'Ctrl+;'`.
7. Capture: `await invoke('hotkey_capture')`, then within 10 s press `Ctrl+Shift+X` in any app. It returns `"Ctrl+Shift+X"` and the keypress does not reach the app.
   Esc alone returns `cancelled`; pressing nothing returns `timed out`, and the next key behaves normally again.

## Without the game (hook check)
- `COMPANIONFRAME_HOTKEY_ANY_WINDOW=1 pnpm tauri dev`: the bindings fire with any window in front (the key is not swallowed). For debugging only.

## `global-shortcut` fallback
There is no clean way to force `SetWindowsHookExW` to fail. If it does fail, the log says `keyboard hook unavailable, falling back to global-shortcut`
and the bindings switch to `RegisterHotKey`: they work, but **they fire and consume the key with any app in front**. Report it if you ever see it.

## What to note in a bug report
Game resolution and mode, Windows version, whether an antivirus flagged the hook, and whether F8 stopped working after minutes of play (hook removed by a timeout).
