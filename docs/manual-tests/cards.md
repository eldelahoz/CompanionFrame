# Same-height cards (Mastery, Relics, Foundry)

Run `pnpm tauri dev` with a real account, or `pnpm dev` and add `?bigMastery=2000`, `?bigRelics=2000` or `?bigFoundry=2617` to the URL for a big list. In each view the cards sit in a virtualized grid over the app scroll, and every card is the same height.

For each of Mastery, Relics and Foundry:

1. **Scroll to the end.** Drag the scrollbar to the bottom and back: the grid never goes blank for more than a frame, the last card is the last item of the sort, and the Elements panel holds only a few dozen cards (`.ms-card`, `.relic-compact-card`, `.fd-card`).
2. **Same height.** All cards in the view are the same height, whatever they show (owned or not, with or without components, rewards, ingredients).
3. **Narrow window.** Drag the window narrower and wider: the number of columns changes, the rows keep their spacing, no card overlaps the next row, nothing is cut off inside a card.
4. **Zoom.** Change the app zoom and the font size in Settings: cards grow and shrink with it (heights are in rem), nothing is clipped, and the grid stays aligned.
5. **Long names.** A long item name takes one line and ends in an ellipsis; hover shows the full name.
6. **Switch views and back.** Scroll to the middle, open Dashboard, return: same scroll position and same rows. Change a filter or the search: the grid goes back to the top with the matching items.
7. **Keyboard.** Tab reaches the card buttons and the pin buttons, Enter opens the details.

Per view:

- **Mastery.** An item with more than six parts shows six dots and "+N" (hover for the rest). An unowned item shows "Not owned" in the same slot as the progress bar of an owned one, and a card without an XP chip keeps its empty slot, so the bars line up across a row. The rank next to the bar reads "24/40" and its hover shows the percent.
- **Relics.** The Vaulted or Unvaulted badge is on the second line next to "Owned N", and the name has the full width of the card. A relic with fewer than six rewards keeps the row's height.
- **Foundry.** Ingredients are one row of chips (image, have/need, border green when owned, amber when only the blueprint is owned, red when missing, plus the check, dot or x in the corner). Open an item with many ingredients (a Warframe set, a Helminth recipe) and narrow the window: the last chip turns into "+N" and its hover lists the hidden ingredients with their state. A blueprint with no recipe data says so in the chips slot. Pin a blueprint: the pinned totals stay above the grid.
- **Foundry doubled ingredients.** A Twin or Ak weapon with one base weapon owned is not ready; it needs two.
- **Mod max rank.** Augur Reach at rank 5 shows 5/5 and the max-rank price (its cap is 5, not 10); the "not max" filter drops it.
