# Contributing to CompanionFrame

## Reporting a problem

Open an issue on the repository's **Issues** tab and pick the bug report or feature request form. Please check existing issues first.

Include:

- App version, and whether you use Windows or Linux.
- Game language and app language.
- Data mode: EE.log + OCR, warframe-api-helper or game memory.
- Steps to reproduce, what you expected and what happened.
- Screenshots, if they help.
- The app log: `companionframe.log.YYYY-MM-DD` in `%LOCALAPPDATA%\com.companionframe.app\logs` (Windows) or `~/.local/share/com.companionframe.app/logs` (Linux).
- The text from Settings > Diagnostics > Copy.

Do not paste tokens, cookies or account ids. Check the log and screenshots before attaching them. For a security problem, follow [SECURITY.md](SECURITY.md) instead of opening a public issue.

## Opening a pull request

1. Fork the repository and branch from `main`.
2. Set up the project as described in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). `pnpm install` also enables the commit-msg hook.
3. Keep the change focused: one concern per pull request, with tests for new logic. User-facing text goes in all five locale files (`src/i18n/{en,es,fr,de,zh}.json`).
4. Run the same checks as CI before pushing:

   ```sh
   pnpm build
   pnpm test
   pnpm bindings && git diff --exit-code src/bindings.ts   # commit src/bindings.ts if it changed
   cd src-tauri
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   ```

   CI (`.github/workflows/ci.yml`) runs them on Windows and Ubuntu 22.04.
5. Open the pull request and fill in the template. Its title follows the commit convention (`type(scope): summary`, at most 140 characters), because it becomes the merge commit title; the `PR title` check enforces it.

`main` only changes through pull requests that pass CI. They are merged with a merge commit, so the branch's commits stay in the history (no squash or rebase), and the branch is deleted after the merge.

CI builds and tests only when a pull request touches code. Changes limited to Markdown files, `docs/`, `LICENSE` or the issue templates skip the build, and the required `ci-ok` check still passes. Releases are never built from a merge: only pushing a `vX.Y.Z` tag starts the Release workflow.

**No Digital Extremes art or trademarks in the repository.** Do not commit game icons, textures, screenshots of the game client, sounds, or DE logos, not even in tests or docs. Game art is loaded at run time from public sources. Test fixtures must be text, synthetic images, or crops that contain no player names, chat or ids.

## Commits

Format ([Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)):

```
type(scope): summary in the imperative
```

- **At most 140 characters in total** (the first line plus the body, if any). If it does not fit, the commit does too much: split it.
- English, lowercase, no trailing period.
- The scope is optional, but use it whenever the change falls in a single one.
- Breaking change: `!` before the colon, e.g. `feat(db)!: drop legacy price table`.
- A `commit-msg` hook (`.githooks/`) enforces this convention; `pnpm install` enables it.

### Types

| Type | For |
|---|---|
| `feat` | A new feature |
| `fix` | A bug fix |
| `refactor` | A code change that does not change behavior |
| `perf` | A performance improvement |
| `test` | Tests only |
| `docs` | Documentation only |
| `build` | Dependencies, Cargo, Vite, packaging |
| `ci` | CI workflows |
| `chore` | Maintenance that fits nowhere else |

### Scopes

| Scope | Covers |
|---|---|
| `frontend` | The React interface (`src/`) in general |
| `backend` | Rust (`src-tauri/`) in general |
| `overlay` | Overlay windows |
| `hotkey` | Global keyboard shortcuts |
| `ocr` | Capture and OCR |
| `scanner` | Reward and riven scanners |
| `inventory` | Inventory and game data |
| `wfm` | warframe.market |
| `eelog` | EE.log and trackers |
| `db` | SQLite and migrations |
| `i18n` | Translations |
| `linux` / `windows` | Platform-specific code |
| `deps` | Dependency updates |

If a new scope keeps coming up, add it to this table.

### Examples

```
feat(overlay): click-through window above the game
fix(hotkey): ignore key auto-repeat
docs(frontend): document the ipc layer
build(deps): bump tauri to 2.9
refactor(wfm)!: split order validators per endpoint
```

## Migrations

A new migration is a named entry added **at the end** of `MIGRATIONS` (`src-tauri/src/db/mod.rs`), with idempotent SQL (`IF NOT EXISTS`).
Never edit a migration that has already shipped; fix forward with a new entry.
