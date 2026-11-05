# F6-T17 – Trade notification overlay: manual test

Automated: `cargo test -p companionframe overlay::trade` (queue, order, stale tokens); `src/overlays/trade/TradeOverlay.test.tsx` (screen, close, dismiss, failure, already-closed match); `src/components/settings/TradeCloseSettings.test.tsx` (the switches and the message box); `cargo test -p companionframe` for the shared close path (`close_all`) and the thank-you rules.

## With the game (borderless, Windows 11) — the point is the click without losing focus
1. `pnpm tauri dev`, start Warframe, sign in to warframe.market in the app, and list an item you can trade (e.g. a mod at a sell price).
2. Complete a real trade of that item. When the trade confirms, the popup appears bottom-right without taking focus; the game keeps keyboard focus (keep moving/typing to check) and does not pause or minimise.
3. The popup shows "Trade Finished" with the partner and, under "Matches your listing", each listing the trade matches with its quantity and price. **Click "Close listing"** with the mouse while the game is in front. Expect: your WFM listing closes (check on the site), the card reads "Listing closed", the game still has focus and the click did not reach the game. If WFM refuses, the card shows "warframe.market did not respond" and the button becomes "Retry".
4. **Dismiss** closes it; with no click it closes by itself after 20 s (the bar at the bottom drains).
5. Two trades within 20 s: the second waits ("+1 more"), and takes over when the first closes (by click or timeout). Order is kept.
6. A trade with no matching listing shows "No Listing Matched" and Dismiss only.
7. Outside the popup box, clicks still go to the game (the rest of the window is click-through).
8. Off switch: untick "Trade overlay" in Settings > Overlays & hotkeys; the next trade shows no popup but the desktop toast and the ledger row still appear.
9. **+rep:** signed in to warframe.market, the popup shows "+rep" next to the partner. Click it: it reads "Rep sent" (or "Rep already given" on a second trade with the same person) and the review shows on their warframe.market profile. Signed out, the button is not shown.
10. **Game language:** set Warframe to French (or any non-English language) and repeat step 2. The popup, the toast and the ledger row appear, and the ledger lists the items by their English names, also for mods and arcanes whose name ends with a rank such as "(RARE RANG 0)". Riven names are not translated yet. Cancel a trade at the confirmation: nothing is recorded.

11. **Auto-close (Settings > warframe.market & presence > "Close matched listings automatically", off by default):** with it off, steps 2-3 behave as before (nothing closes until you click). Turn it on and complete another trade of a listed item: the listing closes by itself on warframe.market, the Market view drops the row, and the popup shows the match as "Listing closed" with no button. A listing WFM refuses (e.g. go offline first) keeps its button, now "Retry". The setting's description warns that warframe.market treats automation as a grey area.
12. **Thank-you message (Settings > warframe.market & presence, off by default):** turn on "Copy a thank-you after a sale" and keep the default text. Sell a listed item and close the listing (click, or automatically): paste in a text field and expect "Thank you for your purchase. I appreciate the +rep <3". Edit the text, leave the field, and repeat: the new text is copied. A sale that closes several listings copies once. The log says a message was copied but never shows its text.
13. **Popup note:** when the thank-you was copied, the popup says "Thank-you message copied. Paste with Ctrl+V."
14. **Nothing copied after a purchase:** put something on the clipboard, then close a listing matched by a purchase (a buy order you filled). The clipboard still holds what you put there. With the switch off, a sale copies nothing either. The text box is greyed out while the switch is off.
15. **Sample popups (dev builds only):** in `pnpm tauri dev`, Settings has a "Developer" card with buttons for One listing, Two listings, Closed + message copied, Purchase, No listing and Plain trade. Each shows the popup with sample data and sends nothing to warframe.market. The card is absent in a release build.

If step 3 steals focus or the click reaches the game, the region/click-through mode of the overlay manager (see f6-t1-overlay-manager.md) is not working as designed: report it as a bug with the Windows version and display scaling.

## Known gaps
- No "open workbench" button: the app has no workbench view to navigate to yet.
- No per-overlay timer setting (fixed 20 s).
- The ledger row is created by the tracker on detection, not by a button; "Close listing" only closes the WFM listing.
- The clickable box position uses `window.screenX * devicePixelRatio`; check it on a 125%/150% scaled and on a second monitor.
