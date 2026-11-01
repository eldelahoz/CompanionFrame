# Riven screens (French client)

Fixtures and expected values: `src-tauri/tests/fixtures/riven/screens/expected.json`.

## Layouts

| Layout | Fixture | Where | Grade |
|---|---|---|---|
| `single-centre` | `../initial-card-orthos-fr.png` | Unveiled/inspected riven, card centred | the card |
| `compare-two` | `compare-orthos-fr.png` (1539×880) | Cycle result: dimmed OLD card left, NEW card right, `CONFIRMER` below | the **NEW** (right, bright) card; optionally show OLD as a comparison |
| `single-left` | `accepted-orthos-fr.png` (1300×774) | After accepting: one card left of centre, `RECYCLER POUR 3.500` below | the card |

Detect compare vs single by finding card frames (two = compare). The NEW card is the brighter, larger, and right-hand one.
Unknown/unsupported layout: keep the "Keep the card visible" hint, but make sure it is only shown when no card was found.

## Weapon: card vs caption
The card says `Orthos`; the "S'ÉQUIPE SUR" caption says `Orthos Prime`. Grade with the card's weapon.
Riven dispositions are per weapon family and the caption only shows the equipped variant, so don't let the caption override the card.
Checked against the data (`the_card_weapon_not_the_equip_caption_is_the_one_the_values_fit`): the read values fit `Orthos` (disposition 1.0), and grading as `Orthos Prime` (0.7) makes the grader log "dispo misfits the rolled values" and regrade as Orthos. So the card weapon is the one graded and the caption is not used.

## Locating the card
`riven_scan::locate_cards` finds the dark text panel with light text in the rows 0.5-0.82 of the 16:9 canvas; `align_rect` moves the layout's crop onto the brightest one (the new card on the compare screen, the dimmed old one scores lower). Without a located card the layout's own crop is used; a read without a weapon is retried on it. The old card is not read (no "vs previous" delta).

## Edge cases
- **Inline damage-type icon**: `+145% [skull]Poison`. The icon sits between the value and label; OCR may drop it or emit junk. Poison = Toxin (`WeaponToxinDamageMod`).
- **Wrapped French labels**: up to 3 lines (`-70,5% de Chances de / Critique pour l'Attaque / Glissée` = `SlideAttackCritChanceMod`). Join continuation lines (those not starting with a sign or `x`) to the previous stat.
- **Multiplier stats** on the old card: `x1,51 points de Dégâts aux Infestés` (factions are multipliers, not %).
- **Decimal comma** (`122,2`), negative values with `-`.
- **Dimmed old card**: low contrast, needs a brighter preprocess or to be skipped.
- **Our own toast** is captured in the screenshot ("Couldn't read the riven. Keep the card visible."): boxes are in `expected.json` `own_toast_box`. Tests/OCR must ignore it. It is in the right half, so it must not be mistaken for a card.
- Capture scaling: boxes are pixels at the stated size; compare as ratios.

## Trimmed screen fixtures

The four PNGs in `src-tauri/tests/fixtures/riven/screens/` keep their original size (the locators and the ratio checks need the whole frame) but are black everywhere except the card boxes of `expected.json` grown by 60 px on each side: `compare-*` keep the old and the new card, `accepted-orthos-fr.png` keeps the single card. Overlay, toast, caption, chat and the rest of the game art were painted black (PNG: 1.8-2.8 MB down to 0.45-0.71 MB). The margin cannot be smaller: with 25 px the locator box of the Staticor card ends 4 px short of the footer pill. This was a one-off Pillow script, so the regions are recorded here instead of a script.

`roll-akbronco-wrapped-fr.png` (1917x1079, inventory screen, French, card with a two-line name and no cycle pill) keeps two regions instead of the card alone: x 600-1340 / y 300-1000 (the card with a wide margin) and x 1480-1917 / y 440-1000 (the Ordis dialog and the item portrait, white text on dark that the card locator once preferred over the card). A tighter 60 px margin around the card hides the locator failure it was cut for.
