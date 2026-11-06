# Inventory performance

Needs a big account (2500+ items). Run `pnpm tauri dev` and open the devtools console (the `timing:` lines only print in dev).

1. **First render.** Open Inventory. It shows within about a second; note `inventory_first_rows ms=` and `parse_inventory ms=`.
2. **One parse.** After the start, `json_parse_inventory` appears once per inventory update, not twice (the lines before were two, one from App and one from the store).
3. **Scroll to the end, cards.** Drag the scrollbar to the bottom and back: the list never goes blank for more than a frame, the last card is the last item of the sort, the DOM holds only a few dozen cards (Elements panel, `.inv-card`).
4. **List mode.** Same in List. Narrow the window under about 750 px: rows become cards, scroll still works to the end.
5. **Select across far rows.** Turn on Select, click the first card, scroll far down, Shift+click another: the count in the selection bar is the whole range, not only what was on screen. Scroll back up: the first cards are ticked.
6. **Keyboard.** Tab through the toolbar into the cards, Enter opens an item, Space/Enter on the checkbox toggles it while Select is on.
7. **Switch views and back.** Scroll to the middle, open Dashboard, return: same scroll position and same rows.
8. **Prices stream.** Sort by Name and watch the prices fill in: the cards do not jump or reorder, the totals strip keeps growing. Sort by Platinum: the order settles within a second of the last prices; `inventory_prices_done sorts=` stays in single digits.
9. **Filters.** Type in the search box while prices are loading: typing stays smooth, the count and the totals follow.
10. **Full Sets tab with "incomplete sets"**: a set card with many missing parts scrolls inside its card instead of growing it.

Compare, before and after, in the console: `inventory_first_rows ms`, `inventory_prices_done sorts`, how many `json_parse_inventory` lines per start or update.
