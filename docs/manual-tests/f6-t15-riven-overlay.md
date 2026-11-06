# F6-T15 – Riven overlay (manual, needs Warframe)

Hotkey: `riven_rescan` (default F6, configurable). Models: `pnpm game-data` + the ONNX runtime/models present (see `f6-t4-onnx.md`).

1. Warframe in borderless, English UI. Open a riven in Inventory (Mods > Riven) so its card is centred.
2. Press F6. A "Scanning..." chip appears, then the card: weapon + overall grade, each stat with value and grade, the sheet's best attributes, and (a moment later) up to 5 similar warframe.market listings.
2b. Open a riven on the cycle screen and do not press F6: the first read happens by itself and, while the card is still drawing, retries every 0.4 s (up to 4 times) before showing "Couldn't read the riven".
3. Check: the game keeps focus and keys still reach it; the overlay is click-through.
4. Press F6 again with the same card open: same window, the old card stays under the chip, then updates. Open a different riven and press it again: the new riven replaces the old.
5. Chat link and roll screens: the backend takes a layout (`riven_overlay_scan` with `chat` or `roll`); the hotkey always uses the centred card.
6. Wait 60 s without pressing anything: the card closes by itself.
7. Rename `resources/onnxruntime.dll` away and press the hotkey: the overlay says the scanner is unavailable and nothing crashes.
8. Press the hotkey with no riven on screen: "Couldn't read the riven" message.
9. Check the overlay does not appear in the captured frame on a rescan (grades must not change between two scans of the same card).

Not covered here: the roll-compare screen (old card against new card, see [../riven-screens.md](../riven-screens.md)) and the Chat/Roll layouts on the hotkey.
