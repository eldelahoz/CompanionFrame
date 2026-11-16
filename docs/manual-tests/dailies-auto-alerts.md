# Dailies tracker: alerts tick themselves

Source: `src/lib/world/dailiesAuto.ts` (`autoTrackerState`), `src/components/world/DailiesTracker.tsx`. Needs a synced inventory.

1. Open Dailies with at least two live alerts in the list. Both rows are unticked.
2. Finish one alert in game, then sync the inventory.
3. That alert's row ticks by itself and shows the auto-tracked tooltip. Clicking it does not untick it.
4. The alert you did not finish stays unticked, and you can still tick it by hand.
5. When the finished alert expires and a new one appears, the new row starts unticked.
