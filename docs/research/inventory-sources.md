# Non-invasive inventory alternatives for CompanionFrame

Read-only research. Nothing in the repo or in `%LOCALAPPDATA%\Warframe` was modified. Personal identifiers (display name, partner names, account id, nonce, IPs) are redacted from this write-up.

**Bottom line:** EE.log never contains inventory contents or item counts. It contains login and sync markers, a few consumable and ammo counts, and trade-dialog text (item display names). The reward and relic lines the repo already parses carry timing only, not item names. A fully non-invasive full inventory is therefore only possible as an imported snapshot plus partial deltas. The memory-free sources are the profile endpoint (which has no counts), a file import, and the GDPR route (unverified). Drift is unavoidable.

---

## 1. What EE.log contains about inventory and ownership

Evidence base:
- A real `EE.log` (248 KB, 2,630 lines, last written 2026-10-26 21:56). It is one login session that never entered a mission, so mission, reward and trade lines are not in it. For those I rely on the repo parsers and test fixtures, and I mark them "from parser/fixture".
- `src-tauri/src/services/eelog_parsers/{lines,trade,timing,arbi}.rs`.
- The original WFHelper project's EE.log monitor, which uses the same trade markers (`TRADE_DIALOG_START`, `TRADE_SUCCESS`).

I grepped the full log for inventory, reward, trade, purchase, foundry, craft, claim, credits, platinum, loadout, sync, relic and mastery. Every hit falls into one of the families below.

### Line families present in the real log (all login or sync, no item deltas)

| Family | Redacted example | What it reveals |
|---|---|---|
| Inventory sync start | `24.505 Sys [Info]: SyncInventoryFromDB` | Marks the start of the full inventory fetch. This is the best "snapshot is now stale or fresh" marker. |
| Inventory payload received | `25.082 Sys [Info]: OnInventoryResults, body size=1005174`<br>`25.470 Sys [Info]: OnInventoryResults completed in 388ms` | Proves a full `inventory.php` arrived. It exposes only the byte size, which is a weak change detector, since a different size means the inventory changed. |
| Server-schema complaints | `25.122 Sys [Error]: Bad data from inventory.php:` followed by `Unknown property: EndlessXP[1].PendingRewards[5]...`, `LoadOutPresets.GEAR`, `OperatorLoadOuts[0].OperatorAmp`, `WeeklyVaultBonusRewards[0].Rewards[1].RewardClaimed` | This leaks the inventory schema key names only, no values. It is noisy and useless for state. |
| Initial give / reset | `15.425 Sys [Info]: Clearing *all* inventory checkpoint info`<br>`16.523 Sys [Info]: Give inventory - no migration data`<br>`16.523 Sys [Info]: GiveInventory() 0ms` | Session reset markers. |
| XP total | `19.881 Sys [Info]: Player has 0 (item based) XP`<br>`25.131 Sys [Info]: Player has 1693600 (item based) XP` | Total item-based mastery XP, a single integer. It rises when mastery items are ranked. |
| Loadout validity | `25.218 Sys [Warning]: Invalid item ID for loadout item at slot SUIT_SLOT`<br>`No items in inventory list for LOT_KDRIVE, SUIT_SLOT; this slot will be empty in loadout!` | The equipped slot references a missing item. Occasional and not a data source. |
| Ship ammo | `25.338 Sys [Info]: Ship ammo synced:`<br>`25.338 Sys [Info]: /Lotus/Weapons/CrewShip/Missiles/MissileAmmo : 41` | Railjack ammo counts. Only 3 to 4 item types, so it is practically irrelevant. |
| Equipped consumables | `31.398 Sys [Info]: Consumable slot 4 - /Lotus/Types/Restoratives/SyndicateTeamEnergyTotem: 63` | The count of the item in each equipped consumable slot only. It is the only real per-item quantity in the log. |
| Login complete | `24.483 Sys [Info]: Logged in <displayName>`<br>`29.069 Sys [Info]: LotusProfileData::CompleteLoginTasks 1 -`<br>`29.078 Script [Info]: ThemedMainMenu.lua: MainMenu::LoginDone result=true` | Session start. `Logged in <name>` also gives the display name (personal). `LoginDone result=true` is the best "inventory is loaded" marker. The repo already has `is_login_complete_line`. |
| Login failure | `20.476 Sys [Error]: Login failed: Login failed; nonce still set` | Stale-session marker. The word "nonce" is in the text, but the value is not. |
| Ownership checks | `29.085 Script [Info]: CheckInventoryOwnership.lua: [CheckInventoryOwnership] /Lotus/Powersuits/Devourer/GrendelPrime is not there. Fail`<br>`CheckItemMastery.lua: [CheckItemMastery] User has 1 maxed items (when needed 5)` | The most interesting family. The game's achievement scripts ask whether an item is owned. 16 lines in this log, all "is not there. Fail". It only reports the failure, so it is a negative-ownership oracle for a small hard-coded list (accolade glyphs, `*Prime` frames, Operator antiques). The success form never appears here, and the list is tied to achievements, not to what you own. |

Not present anywhere in the log: item counts for resources, mods, blueprints or relics, credits, platinum, endo, foundry queue contents, and any account id or token. `OnInventoryResults` logs the size only. A separate check of the OpenWarframe doc found zero accountId or nonce matches in login lines.

### Line families the repo already parses (not present in this session's log; from parser/fixture)

| Family | Marker (from code) | Redacted example | Reveals |
|---|---|---|---|
| Trade confirmation | `Dialog.lua: Dialog::CreateOkCancel(description=Are you sure you want to accept this trade? You are offering:` through `leftItem=/Menu/Confirm_Item_Ok, rightItem=/Menu/Confirm_Item_Cancel)` | `2416.657 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Are you sure you want to accept this trade? You are offering: <ItemName> x<N> ... and will receive from <partner> the following: Platinum x<N>, title= leftItem=...)` | The only line family with real item deltas. It has display names, stack counts and platinum, in both directions. Names are localized and may be truncated. |
| Trade success | `Dialog::CreateOk(description=The trade was successful!, title= leftItem=/Menu/Confirm_Item_Ok)` | `2420.118 Script [Info]: Dialog.lua: Dialog::CreateOk(description=The trade was successful!, ...)` | Commit signal. Without it the buffered dialog is discarded after 60 s. |
| Mission reward trigger | `Sys [Info]: Pause countdown done` or `Sys [Info]: Got rewards` (`is_reward_trigger_line`) | `123.4 Sys [Info]: Got rewards` | A reward screen appeared. It gives no item names, only timing. |
| Relic reward choice | `ProjectionRewardChoice.lua: Missing icon data!` (UI ready) and `ProjectionRewardChoice.lua: Relic reward screen shut down` | `…ProjectionRewardChoice.lua: Relic reward screen shut down` | Relic reward screen open and close. The item identity is not in the log. WFInfo and AlecaFrame OCR the screen for that. |
| Relic picker open | `ThemedProjectionManager.lua: LoadingCompleteEnd` | | Relic selection UI. |
| Mission end | `Sys [Info]: EOM missionLocationUnlocked=` and `TopMenu.lua: Abort:` (`is_mission_end_line`) | `81.780 Sys [Info]: EOM missionLocationUnlocked=1` | Mission end or abort. |
| Mission tag | `activeMissionTag="VoidT6"` | | The mission type, which gives the relic tier. |
| Whisper tab | `ChatRedux::AddTab: Adding tab with channel name: F<user> to index` | | Counterparty for trade attribution. This is a personal identifier. |

### Not found in the log, so treated as absent
- Foundry claims or completion.
- Purchases (market, vendor, Baro, Nightwave, Syndicate).
- Crafting starts.
- Credits or platinum balance changes.
- Resource pickups.
- Per-item drops.

The one weak exception is a full-session bulk signal: `SyncInventoryFromDB` / `OnInventoryResults` body size changing between sessions.

The grep also showed no `Rented`, `Added`, `Removed` or purchase-like lines. The bulk of the log is loader, GFX, network and audio noise, for example 46 `Creating a structured buffer` and 38 `Spot-loading` lines.

**Caveat:** my "absent" list is based on one short login-only session (the log resets each launch). A log from a long session with purchases, crafts and missions could add families. A 30-minute capture session would settle it.

---

## 2. Credential-free DE endpoints and what they expose

I could not call endpoints live from here. The facts below come from the sources cited, so treat them as secondhand.

| Endpoint | Needs | Inventory-related data |
|---|---|---|
| `https://api.warframe.com/cdn/getProfileViewingData.php?playerId=<accountId>` (PC; console hosts `content-ps4/xb1/swi/mob/and.warframe.com/dynamic/...`) | The 24-hex **account id** (the `gid` cookie of warframe.com). Lookup by display name returned 200 with an empty body on 2026-10-16, so it is by id only. Cached about 10 min. | Per the OpenWarframe write-up of the response shape: mastery rank, **`LoadOutInventory.XPInfo[{ItemType, XP}]`** (which items you have ranked, so a **partial ownership oracle**: any item with XP is owned or was owned), `WeaponSkins`, `PlayerSkills`, `Missions` (star chart), `Affiliations` (standing), `LoadOutPreset` (the equipped loadout), `Stats` (lifetime stats), `Wishlist`, `TechProjects`. **No** counts for resources, mods, blueprints or relics, no foundry queue, no credits or platinum. |
| `worldState.php` (and community mirrors) | Nothing | World state only: fissures, invasions, Baro, vault status, Nightwave, Arbitration, sorties. It carries nothing about your inventory. It is useful for context such as "relic tier available", not for ownership. |
| Public Export: `https://origin.warframe.com/PublicExport/index_<lang>.txt.lzma`, then `http://content.warframe.com/PublicExport/Manifest/...` | Nothing | Item catalogs (weapons, warframes, upgrades, recipes, resources, relic tables). Static game data, used to turn paths like `/Lotus/...` into names. It has no player data. |
| `inventory.php` and `login.php` (with `accountId` and `nonce`, or with account password) | A session nonce (from process memory) or the account password | Full inventory. This is exactly the data the current memory-reading path obtains. It requires credentials, so it is **not** credential-free. |

Practical value: the profile endpoint gives a solid **ownership of equipment** (via `XPInfo`) plus mastery and star chart. It does not give counts. The account id is needed and EE.log does not contain it (verified by OpenWarframe: zero matches for accountId/nonce/id patterns). In my own grep of this log, the only identifiers were the display name and IPs. So the id would have to be entered by the user or read from the warframe.com `gid` cookie. That is a user action, not a memory read, but it is still a manual setup step.

---

## 3. How other tools handle it, and DE's stance

| Tool | How it gets data | Reads game memory? | Source |
|---|---|---|---|
| **WFInfo** | OCR of the screen (relic reward and trade/inventory screens) against Warframe Market prices. | No. It captures the screen. | TheGamerCodex: "WFInfo is an open-source desktop overlay that OCRs the Void Relic reward screen during fissure missions." and "it runs in the background reading the screen." https://thegamercodex.com/en/warframe/tools/wfinfo |
| **AlecaFrame** (Overwolf) | Inventory via Overwolf's game-events layer, cached encrypted at `%LOCALAPPDATA%\AlecaFrame\lastData.dat`. Updates only at login and loading screens. OCR for relic rewards. | Overwolf obtains the data. AlecaFrame does not describe the mechanism. I did not verify whether Overwolf reads memory or intercepts the network. | AlecaFrame FAQ G1: "The potentially risky part, getting your account/inventory data, is handled by Overwolf, which is explicitly said to be fine in DE's Third Party Policy" (it links the DE forum post `topic/1383123-third-party-software-usage`). G6: "the data only updates during log-in or loading screens." http://docs.alecaframe.com/faq |
| **Quantframe** (open source Rust/Tauri) | Three source adapters: `profile` (public profile), `file` (plain inventory JSON), `alecaframe` (decrypts `lastData.dat`). | No, per the OpenWarframe write-up. | https://github.com/AES256Afro/OpenWarframe/blob/main/docs/ACCOUNT_DATA_ACCESS.md |
| **warframe-api-helper** (Sainan) | Finds `Warframe.x64.exe`, scans its memory for the session auth string (`?accountId=...&nonce=...`), then calls `inventory.php`. Writes `inventory.json` and `lastData.dat`. | **Yes.** | README: "Scans the process's memory for the session authorization string ... that the game client already holds." and "Using tools that read game client memory may violate the Warframe Terms of Service and could result in action against your account." https://github.com/Sainan/warframe-api-helper |
| **FrameForge** | A memory scanner plus an optional API connection. | **Yes** (basic quantities from the scanner). | "Without the API, basic quantities are still available from the memory scanner." https://wyrmstudios.github.io/FrameForge-Website/docs.html |
| **Warframe Market's own app** | Not verified. I found no source describing how it reads game data. | Unknown. | none |

### DE's public stance
- **DE forum post "Third-Party Software and You"**: https://forums.warframe.com/topic/1320042-third-party-software-and-you/ (mirrored at https://www.reddit.com/r/Warframe/comments/wvr9aq/thirdparty_software_and_you/). Snippet quote, retrieved via search: "The simple, golden rule is this; If you use external software in conjunction with Warframe, then you do so at your own risk." The page itself was behind a bot challenge, so I could not read the full text. I did not try to bypass the challenge. The OpenWarframe write-up summarizes the rest: DE publishes no approved or banned list, because tolerated software can later be caught in an automated ban "drag net", and the same rules apply to Overwolf apps.
- **Reddit thread on AlecaFrame**: https://www.reddit.com/r/Warframe/comments/1blw99u/alecaframe/ . Snippet: "Digital Extremes has said they cannot bless the app but has issued overall statements about what third party apps should and shouldn't do."
- **Overwolf support**: "Overwolf and all official Overwolf apps will NOT get you banned." This is Overwolf's claim, not DE's. https://support.overwolf.com/support/solutions/articles/9000182312-overwolf-won-t-get-you-banned
- **DE post "Third party software usage" (2024)**: https://forums.warframe.com/topic/1383123-third-party-software-usage/ . I could not fetch it. A stealth-mode fetch attempt was blocked and I did not retry. Its content is known only via the AlecaFrame FAQ and OpenWarframe quotes above, so treat the Overwolf-is-fine claim as secondhand.
- **warframe-api-helper's own disclaimer** (above) explicitly says memory reading may violate the ToS.

**Reading:** DE has never published a blanket "memory reading is allowed" statement. Screen capture (WFInfo) and the public profile endpoint (which the game and website use themselves) are not singled out as a problem. Reading `EE.log` is a local, read-only file read that DE does not mention in what I could retrieve. I found no DE text that mentions the EULA clause by number. I did not locate the EULA/ToS text itself, so I cannot quote it.

---

## 4. Event-sourced inventory: feasibility

Design: `state = import_snapshot + Σ deltas from EE.log`, reset whenever a fresher snapshot arrives.

### Possible snapshot sources, ranked by invasiveness
1. **File import of `inventory.json`** or an AlecaFrame `lastData.dat` that the user already has. This is the Quantframe `file` and `alecaframe` pattern. The AlecaFrame file is AES-encrypted. OpenWarframe notes the keys are not public, and Quantframe fetches them from its own backend, so decrypting it is fragile. Using AlecaFrame's own decrypted output via the community decrypter (`https://sainan.github.io/alecaframe-inventory-parser/`, listed on the OpenWF import guide) is possible but is third-party tooling. The AlecaFrame file is only as fresh as the last login or loading screen.
2. **DE GDPR/CCPA data export.** The DE forum thread https://forums.warframe.com/topic/1474298-gdpr-request/ describes only how to file a ticket ("Account" > "CCPA or GDPR"). I found **no evidence** of what the export contains, and no evidence it includes the live inventory or counts. Do not rely on this until someone files a request and inspects the result. It is slow and manual in any case.
3. **Profile endpoint** (`XPInfo`, mastery, star chart) as a partial overlay. It gives owned and ranked equipment, but no counts. It can verify the equipment half of an imported snapshot.

### What EE.log deltas can accurately maintain
- **Trades.** From the dialog, items by display name and stack count, plus platinum, with the success line as a commit. This is accurate when parsed fully, but you must map the localized display name to a `/Lotus/...` path, handle truncated names, and handle cancelled trades. Platinum is the only currency delta in the log.
- **Session boundaries.** `SyncInventoryFromDB` and `LoginDone` mean "the server has the truth now". Anything you tracked is superseded by the next snapshot, which is also when drift resets.
- **Ranked mastery XP** (a single integer from `Player has N (item based) XP`). It is a checksum, not a delta source.
- **Equipped-consumable counts**, a few slots only.

### What would drift or be impossible
- **Mission and relic rewards.** The log has reward timing but no item identity. The relic choice is made on screen. So rewards would have to come from OCR (the WFInfo approach), which is a separate, less invasive but heavier component. Without it the inventory misses every drop.
- **Resources picked up in missions** and mod drops. No log lines.
- **Purchases, crafts, foundry claims, vendor/Baro buys, Nightwave/syndicate redemptions, selling, dissolving mods into endo, Helminth, upgrades, forma.** No lines found in this session's log. They would drift silently.
- **Credits and endo.** No lines.
- **Out-of-session changes** (mobile companion app, website, another PC). Invisible to the log, so the next snapshot is the only repair.
- **Log rotation.** `EE.log` is rewritten on every game start. The previous session survives only if you archive it, so crashes or missed launches lose deltas.

### Verdict
- Event-sourcing works as a hybrid: **trust the snapshot for counts, use deltas only for trades, and show a freshness indicator** ("snapshot from <date>, N trades applied since"). Expect large drift in resource, mod and relic counts after one session of play.
- It is **not** a drop-in replacement for the current memory path. The current path gives exact live counts. The non-invasive path gives exact equipment ownership (profile) and approximate counts (import plus trades, plus OCR of rewards if you add it).
- Easiest wins: (1) the file-import source (the Quantframe `file`/`alecaframe` adapter pattern), (2) the profile endpoint for ownership and mastery, (3) `SyncInventoryFromDB` / `LoginDone` as snapshot-staleness markers, (4) trades from the already-built parser.

### Suggested follow-ups
- Capture a 30 to 60 minute EE.log session that includes a purchase, a foundry claim, a mission with drops, and a relic reward, and re-run this analysis on it. It is the only way to confirm absence of those families.
- Do a live check of `getProfileViewingData.php` with your own account id to confirm the field list (I only have secondhand shape).
- Check whether `SyncInventoryFromDB`'s `body size` correlates with inventory changes between sessions.

## Source index
- https://github.com/Sainan/warframe-api-helper
- http://docs.alecaframe.com/faq
- https://about.openwf.io/import
- https://github.com/AES256Afro/OpenWarframe/blob/main/docs/ACCOUNT_DATA_ACCESS.md
- https://wiki.warframe.com/w/Public_Export
- https://thegamercodex.com/en/warframe/tools/wfinfo
- https://support.overwolf.com/support/solutions/articles/9000182312-overwolf-won-t-get-you-banned
- https://forums.warframe.com/topic/1320042-third-party-software-and-you/ (search snippet only)
- https://www.reddit.com/r/Warframe/comments/1blw99u/alecaframe/ (search snippet only)
- https://forums.warframe.com/topic/1474298-gdpr-request/ (search snippet only)
- https://wyrmstudios.github.io/FrameForge-Website/docs.html (search snippet only)
