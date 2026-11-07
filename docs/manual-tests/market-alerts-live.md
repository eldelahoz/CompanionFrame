# Market alerts: live new-orders feed

Source: `src-tauri/src/services/market_alerts/`, `src-tauri/src/services/wfm/ws.rs`. Needs a warframe.market account.

1. Sign in to warframe.market (Settings). Open Market > Alerts and add an enabled item rule (a busy item, a generous price).
2. Within a few seconds the header shows a "Live" badge next to the health badge. Hovering it says item alerts get new orders as soon as they are posted and existing ones are checked every 5 minutes.
3. Post a matching order from another account, or wait on a busy item: the notification arrives within seconds, and the hit shows in the history.
4. The same order does not notify a second time at the next poll (wait 5 minutes or use "Test").
5. Disable the rule (or delete the last item rule): the badge disappears. Enable it again: it comes back.
6. Sign out: the badge disappears and item rules go back to a re-check every 3 minutes. Sign in again: it returns.
7. Riven rules are unchanged: they never show as live and keep their 3-minute re-check.
