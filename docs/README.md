# Documentation

Start with the [project README](../README.md). Contributors: [CONTRIBUTING.md](../CONTRIBUTING.md) and [DEVELOPMENT.md](DEVELOPMENT.md).

## Understand the code

| Doc | What it covers |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Components, threads, IPC, backend services, frontend, persistence, inventory sources, external data sources, platform layer, security boundaries, data flows |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Setup, commands, testing strategy, recipes (command, migration, UI, i18n key, overlay), commits, disk tips |
| [../DESIGN.md](../DESIGN.md) | Design system: tokens, components, theming |

## warframe.market

| Doc | What it covers |
|---|---|
| [wfm-api.md](wfm-api.md) | Reference for the public warframe.market API v2 and WebSocket, and what is undocumented |
| [wfm-auth.md](wfm-auth.md) | How sign-in works (v1 header mode, webview option), token handling, observed responses |

## Scanners and replay

| Doc | What it covers |
|---|---|
| [ocr-fixtures.md](ocr-fixtures.md) | Reward OCR harness, results per pipeline stage, Latin vs CH model |
| [riven-screens.md](riven-screens.md) | Riven card layouts, weapon and caption rules, edge cases |
| [replay.md](replay.md) | Run your own screenshots through scanners and overlays offline (`CF_SCREENSHOTS`) |

## Research

| Doc | What it covers |
|---|---|
| [research/inventory-sources.md](research/inventory-sources.md) | What EE.log can and cannot tell about the inventory, and the memory-free sources |

## Testing with the game

[manual-tests/](manual-tests/): checklists for everything that needs Warframe running (overlays, hotkeys, capture, ONNX, reward and riven scanners, the Linux build, the installer, auto-update, warframe.market web sign-in). The file prefixes (`f6-t…`) are historical task ids.

## Performance

| Doc | What it covers |
|---|---|
| [perf/how-to-measure.md](perf/how-to-measure.md) | Measuring CPU, memory and I/O with `scripts/perf/measure.ps1` |
| [perf/budgets.md](perf/budgets.md) | Resource and latency budgets |
