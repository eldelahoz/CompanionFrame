# F6-T11 – Relic planner overlay (manual test with Warframe)

The planner is the reward overlay window in `planner` mode. Borderless windowed, Windows 11, English UI.
Needs the inventory loaded (Settings > inventory source) and Windows OCR with the English language pack.

## 1. Opens on the relic selection screen
1. In a mission-less orbiter session, open the void relic picker (Navigation > a fissure node > choose relic).
2. Within ~1 s the planner appears at the top right with "Reading the era…", then the era name and up to 8
   owned relics of that era, best ducat EV first, each with the refinement the EV is for and its copy count.
3. The game keeps focus and keeps receiving keys; the panel is click-through.

## 2. Era per fissure type (repeat for each)
| Fissure | Expected header |
|---|---|
| Lith / Meso / Neo / Axi | that era; only its relics listed |
| Requiem | Requiem; only Requiem relics |
| Omnia (VoidT6) | Omnia; every owned relic but Requiem |

Note the reading source per case in `%APPDATA%/com.companionframe.app/logs`: a log tag (`activeMissionTag=VoidT?`)
should win; an Omnia screen must override a stale tag through the **filter tab** read (candidate `filter-label`).

## 3. Degradation
- Rename/disable the English OCR language pack (or run with the game minimised): the panel says
  "Era not detected" / "no OCR" and lists every owned relic; no error toast, no stuck panel (it hides after ~4.5 s).
- Open the picker with an inventory that has no relic of that era: "No relics found".

## 4. Closing and reopening
1. Back out of the picker (Esc): the panel clears within ~1 s (EE.log `InitMapping` line).
2. Opening it again within 3 s of closing must **not** show the panel; after 3 s it must.
3. Entering the picker must not close it by itself (the `InitMapping` line right after the open is entry noise).
4. Idle with the panel up: it hides by itself after 2 min (20 s while still reading the era).

## 5. Interaction (needs the F7 overlay move/interaction mode)
- Toggle interaction: the squad select (Solo/2P/3P/4P) and "Close" appear. Changing the squad re-ranks the list
  without the screen being re-read (no flicker of "Reading the era…", no extra capture in the log).
- "Close" hides the panel and mutes EE.log reopens for 3 s.

## Not covered here
Plat EV (needs the WFM price source), the desktop "push filters to overlay" button, and the auto-trigger setting (T18).
