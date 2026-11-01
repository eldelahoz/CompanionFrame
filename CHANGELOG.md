# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/).

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
