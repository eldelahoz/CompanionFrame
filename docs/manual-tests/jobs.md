# Background jobs

Needs a signed-in warframe.market account with a few sell orders (use orders you do not mind hiding: hiding and showing is harmless). Run with `pnpm tauri dev`.

1. Market > Sell. Tick 10 or more orders and click Hide. The bar stays, the rows flip to hidden one by one and a pill appears in the titlebar: "Hiding 3 of 12...". No progress UI is left in the bar.
2. Switch to another view (Inventory). The pill stays and keeps counting. Come back to Market: the rows are in step.
3. Click the pill. The panel lists "Hide or show orders" with "n of 12", a progress bar and Stop.
4. Start another hide while the first runs: the bar buttons are disabled until it ends.
5. Click Stop while it runs. The button reads "Stopping..."; the order in flight finishes, no other starts. The job ends and the pill shows "x done" for about 5 seconds, then goes away. Show the same orders again with Show.
6. Close the app in the middle of a run (window X, or end the process). Reopen it: nothing runs by itself. The pill reads "Interrupted: x of 12"; the panel says "n left. Nothing resumes by itself." Click "Continue with the n": a new job runs only the orders that never ran. Check that the order that was in flight is not duplicated or lost.
7. Failures: delete an order on warframe.market.com in the browser, then in the app (without refreshing) hide a selection that includes it. The job ends with "n done, 1 failed"; the panel lists "Item name: reason" for the failed one. Click "Retry the 1": a new job runs only that order (and fails again, with its reason). Click Dismiss.
8. Sign out in another place or let the session expire in the middle of a job: the job ends "Interrupted" and Continue works after you sign in again.
9. Reprice: Market > Sell > "Update prices", load prices, click Update. The modal closes with a "Started" note and the pill shows "Updating prices x of n...". Stop and Retry behave as above.
10. Delete: select 2 test orders, Delete, confirm. The selection clears and the rows disappear as the job goes.
11. Workbench: run a small sell plan. The pill mirrors its rows; Stop in the panel cancels like the Workbench's own Cancel. The Workbench view keeps its own review and results.
12. Open Inventory (a large one). The Inventory view has no "Pricing x/y" line any more: the titlebar pill shows a muted "Prices 585 of 1493" (grey, no spinner), and it appears right when the view opens. Switch to another tab: its rows get their prices first. Click the pill: the panel lists "Prices 585 of 1493" with a progress bar and no Stop button.
13. While the prices load, start a hide in Market: the pill switches to the hide job (spinner). When it finishes, the muted Prices pill comes back if the load is still running, and the pill disappears when everything is priced.
14. Cut the network (or sign out of the internet) in the middle of a load: after a few failures the Inventory view still shows its failed count with a Retry button (the pill has no retry for prices); Retry asks again for just those slugs and the pill reappears.
