# Manual test: shared prices (Inventory, Relics, Mastery)

Needs the real app, signed in, with an inventory loaded and an empty or old price cache (restart after a day, or delete `wfm:price:*` rows).

1. Open Relics. Reward prices and the EV fill in without a freeze. The titlebar pill shows the Prices job (muted, no Stop) and goes away when done.
2. Open the relic detail modal: reward prices are already there (same store, no second load).
3. Open Mastery, roadmap tab. Platinum estimates fill in and the "pricing" state ends; the pill shows the Prices job. Switch tabs and come back: prices are there at once.
4. Open Inventory. The average (7-day median) column and the card median fill in for every priced row, with no Load button. Trend arrows appear where price and median differ by more than 10 %.
5. Wait for a background refresh (or restart past 6 h): Relics, Mastery and Inventory values update without reopening the views.
6. Go offline during a load: rows settle as a retry state (Inventory) and the pill pauses; back online, Retry fills them.
