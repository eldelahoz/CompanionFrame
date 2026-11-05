# Development

How the code is organised is in [ARCHITECTURE.md](ARCHITECTURE.md). Commit and pull request rules are in [CONTRIBUTING.md](../CONTRIBUTING.md).

## Setup

Windows is the main development platform.

1. **Rust** (stable, edition 2024) via [rustup](https://rustup.rs) with the **MSVC** toolchain (`x86_64-pc-windows-msvc`) and the Visual Studio Build Tools "Desktop development with C++" workload. WebView2 ships with Windows 11 (Windows 10 needs the [Evergreen runtime](https://developer.microsoft.com/microsoft-edge/webview2/)).
2. **Node 22** and **pnpm 11**.
3. Install and fetch the generated assets:

```sh
pnpm install            # also sets git core.hooksPath to .githooks (commit-msg hook)
pnpm game-data          # copies warframe-public-export-plus + @wfcd/items into src-tauri/resources/game-data
pnpm fetch-onnx         # downloads onnxruntime.dll and the OCR models (SHA-256 pinned) into src-tauri/resources
pnpm bindings           # regenerates src/bindings.ts from the Rust commands
```

`game-data` and `fetch-onnx` outputs are git-ignored; `pnpm tauri dev` and `pnpm tauri build` run both automatically (`beforeDevCommand` / `beforeBuildCommand` in `src-tauri/tauri.conf.json`). `game-data` skips the copy when the package versions are unchanged.

Linux needs the system packages listed in `.github/workflows/ci.yml` (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libdbus-1-dev`, `libasound2-dev`, …). See `docs/manual-tests/f7-t1-linux-build.md`.

## Commands

| Command | What it does |
|---|---|
| `pnpm tauri dev` | Full app: Vite on :1420 + Rust, hot reload for the frontend |
| `pnpm dev` | Frontend only in a browser. IPC is mocked (see below) |
| `pnpm build` | `tsc` + `vite build` (both entries) |
| `pnpm test` | vitest (jsdom) |
| `pnpm bindings` | Regenerate `src/bindings.ts` headlessly |
| `pnpm replay` | Run your own game screenshots through the scanners and overlays offline ([replay.md](replay.md)) |
| `pnpm tauri build` | Release bundle (NSIS on Windows, per `tauri.windows.conf.json`) |
| `cargo test` | Backend tests (run in `src-tauri/`) |
| `cargo fmt --check` / `cargo clippy -- -D warnings` | Lint gates CI enforces |

Local limits: `vite.config.ts` caps vitest at 50% of the cores; to cap cargo too, set `jobs` in a local, gitignored `.cargo/config.toml`.

CI (`.github/workflows/ci.yml`, Windows and Ubuntu 22.04) runs: `pnpm build`, `pnpm test`, fmt, clippy, `cargo test`, then `pnpm bindings` and `git diff --exit-code src/bindings.ts`. Run the same before pushing.

## Testing strategy

- **Rust (`cargo test`)**: logic is kept pure and tested in `tests.rs` or `#[cfg(test)]` beside the module. EE.log parsers are fed fixtures in `src-tauri/tests/fixtures/eelog/` (real log excerpts are the parsers' spec). The overlay manager runs against a fake `Host`. The WFM client is tested against a scripted local HTTP server (`wfm/mock.rs`); nothing in the suite touches the real network. Tests needing `onnxruntime.dll` are `cfg(windows)`. `tests/no_wfhelper_hosts.rs` guards the "no deployed backend" rule.
- **Frontend (`pnpm test`, vitest + Testing Library)**: tests sit next to the code (`*.test.ts(x)`), run in jsdom against the ipc mocks. `src/lib/i18n.test.tsx` fails if a locale lacks a key from `en.json`.
- **UI against mocks (Playwright MCP)**: `pnpm dev` serves the app at `http://localhost:1420` without Tauri; `ipc.ts` then returns mock data, so the Playwright MCP tools can drive and screenshot every view. Overlays are not mocked: `overlay.html` uses Tauri events directly, so test them with `pnpm tauri dev` or vitest.
- **Manual tests with the game**: anything that needs Warframe (overlay focus and click-through, hotkeys, capture, OCR, EE.log trackers, the Linux build) has a checklist in `docs/manual-tests/`, one file per task (e.g. `f6-t1-overlay-manager.md`). Run `pnpm tauri dev` with the game in borderless mode. Add or update the checklist when you change that behaviour.

## Recipes

### Add a command

1. Write the function in the right `src-tauri/src/services/<domain>/`:
   ```rust
   #[tauri::command]
   #[specta::specta]
   pub async fn my_thing(db: State<'_, Db>, input: MyInput) -> Result<MyOut, String> { … }
   ```
   Types crossing IPC derive `serde::{Serialize, Deserialize}` and `specta::Type`. If `input` comes from the WebView, make it a struct with `#[serde(deny_unknown_fields)]` and validate ranges before use (see `wfm/validators.rs`). Keep blocking work in `spawn_blocking`.
2. Add it to `collect_commands![]` in `specta_builder()` (`src-tauri/src/lib.rs`). Managed state it needs is `app.manage`d in `setup()`.
3. Run `pnpm bindings` and commit `src/bindings.ts`.
4. Expose it in `src/lib/ipc.ts` with a mock branch for `!inTauri()`; call that from views/stores, never `commands` directly.
5. If it emits an event, add the `onXxx` wrapper in `ipc.ts`; a payload type used only by an event is registered with `.typ::<T>()` in `specta_builder()`.
6. Test the Rust logic and the ipc mock path.

### Add a migration

Append **one named entry at the end** of `MIGRATIONS` in `src-tauri/src/db/mod.rs`:

```rust
("my_table", "CREATE TABLE IF NOT EXISTS my_table (id TEXT PRIMARY KEY, …)"),
```

SQL must be idempotent (`IF NOT EXISTS`). Never edit or reorder an applied entry; fix forward with a new one. Add the table to the list in `migrations_are_idempotent` if it is new.

### Build UI

The design system is in [`DESIGN.md`](../DESIGN.md).

- **Components first.** Build screens from `src/components/ui` (`Button`, `IconButton`, `Input`, `SearchInput`, `Select`, `Switch`, `Checkbox`, `Tabs`, `Segmented`, `Card`/`CardHeader`, `PageHeader`, `Badge`, `Chip`, `Readout`, `StatStrip`, `Table`, `Modal`, `Toast`, `EmptyState`, `Skeleton`). Icons: `lucide-react` for controls, `NavIcon` for sidebar views, `GameIcon` / `RelicIcon` / `PolarityIcon` (`src/components/icons`) for game data.
- **Tokens only.** Colours, radii, shadows, spacing and motion come from `src/styles/tokens.css` (Tailwind: `bg-bg-surface`, `text-text-secondary`, `rounded-[var(--radius-md)]`, …). Never hard-code a hex value: the theme editor rewrites the tokens at runtime and user custom CSS targets them.
- **Every currency value is a `Readout`** (platinum, ducats, endo, credits), so numbers line up.
- **Responsive by container.** Use `Table mode="stack"` or `mode="scroll"` for tables and container queries for cards; the view must work from an 800×600 window and inside a pop-out window up to ultrawide. Sizes are rem so the font scale, `ui_scale` and Windows display scaling compose.
- **Accessible by default.** Icon-only buttons need `label` (it is also their tooltip); keep the focus ring; no colour-only meaning.
- **No game art in the repo.** Icons and images of Warframe items load at run time from public sources and are cached locally (see the rule in [CONTRIBUTING.md](../CONTRIBUTING.md)). Never commit DE art.

### Add an i18n key

Add the key to **all five** of `src/i18n/en.json`, `es.json`, `fr.json`, `de.json`, `zh.json` (the vitest check fails otherwise). `MessageKey` comes from `en.json`, so use it as `tr("area.key")`; placeholders are `{name}` and are filled with `tr("area.key", { name })`. Do not translate game item names: they come from DE's per-language dictionaries (`game_locale.rs`).

### Add an overlay kind

1. Rust: add a variant to `OverlayKind` and `ALL` / `name()` in `services/overlay/manager.rs`. The host builds one hidden window `overlay-<kind>` per kind (`host.rs`, URL `overlay.html?kind=<kind>`).
2. Capability: add `src-tauri/capabilities/overlay-<kind>.json` (copy an existing one: window `overlay-<kind>`, only `core:event:default`).
3. Backend flow: a module `services/overlay/<kind>.rs` with the view type, an event constant, and show/hide logic; register commands (`lib.rs`) and its tracker in `trackers::setup` if EE.log drives it. Register the view type with `.typ::<…>()`.
4. Frontend: add the kind to `OverlayKind`/`OVERLAY_KINDS` (`src/overlays/common/overlayTheme.ts`) and `OVERLAY_LAYOUT_KINDS` (`config/shared/overlayLayout.ts`), a component under `src/overlays/<kind>/`, and an entry in `SCREENS` (`src/overlays/screens.tsx`).
5. Settings: enabled/opacity are keyed by kind name and validated against the known kinds (`overlay/settings.rs`); add i18n labels and the toggle in `OverlaySettings.tsx`.
6. Add a `docs/manual-tests/` checklist.

## Timing lines

The backend logs `timing:` lines to the daily log file and the terminal (`pnpm tauri dev`), for before/after comparisons: `my_orders` (`http_ms`, `catalog_ms`: near 0 once the catalog is in memory), `wfm_prices_bulk`, `wfm_prices_refresh worker` (one line per drained queue), and at debug level `wfm_request priority= path_kind= wait_ms= ms=` per request (path kind is the first path segment, never an id) and `my_orders_fetch`. Set `RUST_LOG=debug` to see the debug ones.

## README screenshots

`node scripts/screenshots.mjs [outDir]` rewrites the PNGs in `docs/img/`. It starts Vite on port 14330, drives the installed Edge through `playwright-core` (no browser download) at 1440x900, dark theme, English, on the in-browser mocks of `src/lib/ipc.ts`, and stops the server when done. The relic reward overlay runs on a stub Tauri bridge over a neutral backdrop. Change the mock data, not the script, if a shot shows something wrong; the data must stay fictional (no real names, paths or tokens). Icons load from the wiki at runtime, so run it online and check each PNG before committing.

## Commits

`type(scope): summary` in English, lowercase, imperative, **140 characters max in total**, enforced by `.githooks/commit-msg` (installed by `pnpm install`; `.githooks/test-commit-msg.sh` tests it). Types: feat, fix, refactor, perf, test, docs, build, ci, chore. Scopes and examples are in [CONTRIBUTING.md](../CONTRIBUTING.md). Merge, revert, fixup and squash commits are exempt. One concern per commit; if the message does not fit, split the commit.

## Disk tips

Rust build output is large (`src-tauri/target`, easily tens of GB with several worktrees).

- `Cargo.toml` already sets `[profile.dev] debug = "line-tables-only"` and no debug info for dependencies.
- Reclaim space with `cargo clean` in `src-tauri/` (or delete `src-tauri/target/debug/incremental`), and `cargo clean --release` for release artifacts. The next build is a full rebuild.
- Share one target dir between worktrees with `CARGO_TARGET_DIR` if you do not build them concurrently.
- Generated and downloaded assets (`src-tauri/resources/game-data`, `onnxruntime`, `riven-ocr`, `dist`, `node_modules`) are re-creatable with `pnpm install`, `pnpm game-data`, `pnpm fetch-onnx`, `pnpm build`.
