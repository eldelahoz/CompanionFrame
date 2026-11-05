# Notice

CompanionFrame is released under the [MIT License](LICENSE). This file lists the projects, data and services it builds on, and under which terms.

## Trademarks and game content

Warframe and Digital Extremes are trademarks of Digital Extremes Ltd. CompanionFrame is an unofficial, fan-made tool. It is not affiliated with, endorsed by or sponsored by Digital Extremes.

Game art, icons and other Digital Extremes assets are **not distributed** with this project. Item images and icons are loaded at run time from public sources (such as `cdn.warframestat.us` and DE's own public endpoints) and cached on your PC. They remain the property of Digital Extremes Ltd. The MIT license of this repository does not cover them.

## Origin

- **[WFHelper](https://github.com/WFHelper/WFHelper)** (MIT, Copyright (c) 2026 WFHelper). CompanionFrame began as a port of WFHelper to Tauri and keeps its copyright notice in [LICENSE](LICENSE). The first OCR models used by the riven scanner were published in WFHelper's repository (see below).

## Game data

- **[@wfcd/items](https://github.com/WFCD/warframe-items)** (MIT): item, relic and reward data, bundled at build time and refreshed from npm / jsDelivr. Drop tables come from [WFCD's warframestat](https://drops.warframestat.us) service.
- **[warframe-public-export-plus](https://github.com/calamity-inc/warframe-public-export-plus)** (MIT): a packaging of Digital Extremes' PublicExport data, bundled at build time. DE's PublicExport itself is also fetched live from `content.warframe.com`.
- **[WARFRAME Wiki](https://wiki.warframe.com)**: `src-tauri/data/dojo_research.json` (from `Module:Research/data`) and `src/data/codexScanRequirements.json` (from `Module:Enemies/data/*`, plus DE PublicExport) are derived from wiki data, licensed [CC BY-SA](https://creativecommons.org/licenses/by-sa/3.0/). Those files stay under CC BY-SA; the wiki's text is by its contributors. The wiki's images are Digital Extremes' and are not used here.
- **arbi.guide**: the arbitration minimap placement table `src/data/arbiMinimaps.json` (coordinates only, no images).
- **browse.wf** (`arbys.txt`, oracle): the arbitration schedule and a world-state fallback.
- **Riven good-roll sheet** by the 44bananas community, read at run time.
- **AlecaFrame**: the app can read AlecaFrame's local `lastData.dat` cache. The fixed key in `inventory/aleca.rs` belongs to that public file format, not to a secret of this project. AlecaFrame is a separate product.

## Services

- **[warframe.market](https://warframe.market)** API and WebSocket: orders, prices and presence, used under the [API rules](https://docs.warframe.market/docs/rules/overview/) (identifiable User-Agent, rate limits, no automated trading; the only automatic write is the opt-in setting that closes the listings a finished trade matches, off by default). Market data belongs to warframe.market and its users. CompanionFrame is not affiliated with warframe.market.
- **[warframe-api-helper](https://github.com/Sainan/warframe-api-helper)** by Sainan (Unlicense): an optional helper, downloaded only if you enable game-memory mode, from its official release at a pinned version, SHA-256 checked. It is **not redistributed** with CompanionFrame.

## OCR and runtime

`pnpm fetch-onnx` (`scripts/fetch-onnx.mjs`) downloads these at build time, with pinned SHA-256 hashes. They are bundled into the installer and are not stored in this repository.

| Component | License | Source |
|---|---|---|
| ONNX Runtime 1.31.0 (`onnxruntime.dll`) | MIT, Microsoft | [microsoft/onnxruntime](https://github.com/microsoft/onnxruntime) releases |
| PP-OCRv5 Latin recognizer and dictionary (ONNX export) | Apache-2.0, PaddlePaddle / PaddleOCR; export by RapidOCR v3.4.0 | [RapidAI/RapidOCR](https://github.com/RapidAI/RapidOCR) |
| PP-OCRv3 `ch` recognizer and dictionary (used for non-Latin languages) | Apache-2.0, PaddlePaddle / PaddleOCR | copied from WFHelper's repository at a pinned commit; no official RapidOCR or PaddleOCR download with the same SHA-256 was found, so its exact provenance is unverified |
| PP-OCRv5 mobile text detector (ONNX export; finds the riven stat lines) | Apache-2.0, PaddlePaddle / PaddleOCR; export by RapidOCR v3.4.0 | [RapidAI/RapidOCR](https://github.com/RapidAI/RapidOCR) |

On Windows, text is also read with the Windows.Media.Ocr API that ships with the OS.

The default notification sound (`src-tauri/assets/notification.wav`) is original: it is synthesized by `scripts/make-notification-sound.mjs` and is licensed under the MIT license.

## Code dependencies

Rust crates and npm packages are listed in `src-tauri/Cargo.lock` and `pnpm-lock.yaml`. Notable ones: [Tauri](https://tauri.app) (MIT / Apache-2.0), React (MIT), Vite (MIT), Tailwind CSS (MIT), lucide-react (ISC), the `ort` crate (MIT / Apache-2.0), SQLite through `rusqlite` (MIT, SQLite is public domain). Each keeps its own license. A full dependency license audit has not been run.
