# F6-T9/T10: reward overlay (needs Warframe, borderless, Windows 11)

Preview without the game: `pnpm dev`, open `overlay.html?kind=reward&mode=preview`.

1. Start a relic fissure. When the reward screen shows, **without pressing anything** the card row appears over the game's cards
   (EE.log "Got rewards" -> 650 ms -> capture). Note the time from the screen drawing to the card (target < 2 s).
2. Check per card: platinum, ducats, `Own x/y`, set progress/price, "In foundry" (queue a part's blueprint first), correct order left to right,
   with 1, 2, 3 and 4 players (squad of N). Alignment with the game's cards at your resolution: note offsets (layout is editable in the editor).
3. The card closes by itself near the end of the 15 s vote, or ~1.5 s after the game closes its reward screen (min. 5 s visible).
4. Hotkey (default F8) on the reward screen: scans and shows. F8 again closes. F8 right after an auto-close scans again.
5. Let EE.log fire on a plain pause ("Pause countdown done" with no reward screen): no card must open.
6. Interaction hotkey (F7) while the card shows: the close button appears, the card stays until closed; the game keeps focus.
7. Switch game language to FR/DE: the card shows English names, prices still resolve.
8. Check `reward_scan_debug` from devtools (`__TAURI_INTERNALS__.invoke('reward_scan_debug')`) after a miss: OCR candidates per slot.
