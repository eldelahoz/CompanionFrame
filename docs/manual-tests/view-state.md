# View state kept across view switches

Each view comes back as you left it while the app runs. Nothing is saved to disk: closing the app resets all of it.

For every step: change the state, open another view in the sidebar, come back.

1. **Inventory**: pick the Mods tab, type a search, choose a goal, tick Tradable and Surplus, sort descending. All of it is back, including the goal's filters.
2. **Market > My Orders**: switch to Buy and select two orders. Leave and return: same tab, same two orders selected. Delete one of them on warframe.market (or in-app from another view), return: only the existing one stays selected.
3. **Market > Browse**: search an item and open it, set Buyers, status On site, a min price, then the Stats view with a 30 day period. Return: the item is selected, its orders load again and the filters are as left.
4. **Stats > Personal**: open a section, search, sort by a column, go to page 2. Return: same section, search, sort and page.
5. **Relics** (open the Filters button on a narrow window), **Mastery** search, **Rivens** (inventory search, type and grades; sessions search), **Market Analysis** (range, search, type filter, page), **Missions** (mission type, search, expanded rows, period items), **Wiki** (query and mode; the search runs again), **Settings** (search and section): each is back as left.
6. **World**: the Arbitration schedule, Baro, Nightwave and dailies searches are back.
7. Close the app and open it again: every view starts at its defaults. Settings kept by design (sort order, view mode, Inventory price range, World tab) still come back as before.
8. **Market > My Orders search**: with a few dozen orders, type part of an item name (try capitals and an accent-less version of an accented name, and its English name on a non-English game language). The table filters as you type; with no match the line "No orders match ..." shows. Switch Sell/Buy: the text stays. Select all ticks only the matching orders; Hide, Show, Delete and Reprice from the bar touch only the ticked orders still shown (clear the search: the others are untouched). The "Hide all"/"Show all" button still changes the whole side; its tooltip says so. Leave Market and return: the search is empty.
