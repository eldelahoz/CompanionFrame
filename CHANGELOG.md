# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/).

## 0.5.3 (2026-11-09)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Fixed
* fix(inventory): mods use the game's max rank, and unranked equipped mods show as equipped by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/46


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.5.2...v0.5.3

## 0.5.2 (2026-11-08)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Fixed
* fix(foundry): a doubled ingredient like Twin Krohkur's two Krohkurs needs two owned copies, not one by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/44


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.5.1...v0.5.2

## 0.5.1 (2026-11-07)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Fixed
* fix(wfm): treat 509 too many connections like a 429 rate limit by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/41
* fix(alerts): item alerts react to new orders from the warframe.market socket feed, with a Live badge by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/43


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.5.0...v0.5.1

## 0.5.0 (2026-11-07)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Added
* feat(inventory): find surplus copies and where mods and arcanes are equipped by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/23
* feat(overlay): close matched listings automatically and copy a thank-you after a sale by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/25
* feat(market): sort and bulk-edit orders, and tag inventory items you have on sale by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/27
* feat(jobs): run long actions as background jobs with a titlebar pill, stop, continue, retry and an offline pause by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/34
* feat(frontend): keep each view's filters, search, tab and selection when switching views by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/33
### Fixed
* fix(eelog): translate localized trade names that carry a rank suffix by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/24
* fix(stats): show trade item thumbnails from local art instead of the blocked warframe.market host by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/29
* fix(overlay): retry the first riven read and fill relic planner ducats from the item database by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/32
### Performance
* perf(db): WAL, a writer plus three readers, batched price reads and a backup before migrating by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/28
* perf(wfm): one gateway for every warframe.market request, catalog in memory and cached orders by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/31
* perf(inventory): one shared inventory parse, coalesced price updates and virtualized cards and list by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/35
* perf(prices): the worker computes the 7-day median and Relics, Mastery and Inventory share one price store by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/36
* perf(cards): virtualized Relics, Mastery and Foundry grids with same-height cards by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/37


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.4.1...v0.5.0

## 0.4.1 (2026-11-05)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Fixed
* fix(eelog): parse ee.log and dbwin trade lines separately by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/21


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.4.0...v0.4.1

## 0.4.0 (2026-11-01)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Fixed
* fix(backend): quit when the main window closes without tray mode by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/19


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.3.1...v0.4.0

## 0.3.1 (2026-11-01)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Fixed
* fix(app): stop the crash on start by picking the rustls crypto provider by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/16


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.3.0...v0.3.1

## 0.3.0 (2026-11-01)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Added
* feat(updater): check for updates and install them from the app by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/14

## New Contributors
* @dependabot[bot] made their first contribution in https://github.com/eldelahoz/CompanionFrame/pull/12

**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.2.0...v0.3.0

## 0.2.0 (2026-11-01)

<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Fixed
* fix(trade): detect trades in any game language and add a +rep button by @eldelahoz in https://github.com/eldelahoz/CompanionFrame/pull/8


**Full Changelog**: https://github.com/eldelahoz/CompanionFrame/compare/v0.1.0...v0.2.0

## [0.1.0] - 2026-10-31

First public release.

### Added

- **Inventory and mastery**: import an inventory JSON, AlecaFrame's cache or a public profile id; the snapshot is updated by EE.log trades and confirmed relic picks. Inventory, foundry, resources, mastery and syndicates views.
- **Relics and drops**: relic planner data, drop-table search and a Wiki view with ducat and platinum values.
- **World state**: cycles, fissures, Baro (planner and wishlist), Varzia, arbitration schedule, and alerts.
- **warframe.market**: sign-in (password or the site's own page), orders, riven auctions, owned-riven decoding, listing warnings, price history, alerts, bulk-sell workbench, presence over WebSocket.
- **Trade ledger** fed live from `EE.log`, with GDPR export import.
- **Run analysis** for arbitrations and Profit-Taker, and a missions view.
- **Overlays** (Windows): relic reward scan and relic planner, riven scan and grading, arbitration live counter, trade notice. Configurable hotkeys (low-level hook with a global-shortcut fallback) and layout editor.
- **OCR**: Windows OCR plus PaddleOCR through ONNX Runtime (downloaded at build time, SHA-256 pinned).
- **Notifications**: toast, sound, Discord and generic webhooks.
- **Data modes**: EE.log + OCR by default (no game-memory access); optional warframe-api-helper and game-memory mode behind explicit consent.
- **Languages**: English, German, Spanish, French and Chinese.
- **Platforms**: Windows 10/11 (NSIS per-user installer); Linux builds and runs the non-overlay parts.
- Local-only design: no CompanionFrame server and no telemetry.
