# F6-T7 – reward matching and localized aliases (manual, with the game)

The matcher is pure and unit-tested; this checks it against real cards in a non-English client.
Needs T6 (OCR reads) and T9 (orchestration) to drive it end to end; until then, feed the
OCR strings below to `rank_reward_candidates` through a scratch test.

1. Game language French (Options > Interface > Language), open a Void fissure reward screen
   with `Volt Prime - Châssis` and a Forma. Expected: the English `Volt Prime Chassis Blueprint`
   and `Forma Blueprint` come back, never the French text.
2. Same with a `2X Forma` card. Expected: `2X Forma Blueprint` (the card prints `2 X Forma (Schéma)`).
3. German client: `2X Forma` and plain `Forma` cards resolve to their own English names.
4. Change the game language in CompanionFrame settings without restarting, scan again.
   Expected: aliases follow the new language.
5. Chinese clients are not covered by this checklist.
