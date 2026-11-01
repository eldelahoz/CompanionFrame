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
5. Open the pull request and fill in the template. Its title follows the commit convention (`type(scope): summary`, at most 140 characters), because it becomes the merge commit title; the `PR title` check enforces it, and a second job then sets the matching label on the pull request (`feat`, `fix`, `perf`, `docs`, `ci`, `chore`, `test`, `build` or `refactor`, plus `breaking` when the title has `!`), replacing the old one if you edit the title. Pull requests from forks are labelled by a maintainer. The release notes are built from these labels.

`main` only changes through pull requests that pass CI. They are merged with a merge commit, so the branch's commits stay in the history (no squash or rebase), and the branch is deleted after the merge.

CI builds and tests only when a pull request touches code. Changes limited to Markdown files, `docs/`, `LICENSE`, the issue templates or the release and automation files (the release, release-please and PR title workflows, Dependabot and the release-please config) skip the build, and the required `ci-ok` check still passes. The release-please PR and its merge skip it too: they only bump versions and the changelog of code `main` already tested, and the Release workflow compiles it anyway. An ordinary merge never builds a release: only merging the release-please PR (or pushing a `vX.Y.Z` tag) starts the Release workflow (see Releasing).

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

## Releasing

Releases are driven by [release-please](https://github.com/googleapis/release-please) from the Conventional Commit PR titles on `main` (`feat` bumps the minor version while we are 0.x, `fix` and `perf` the patch). The changelog lists merged pull requests, not commits, using GitHub's generated release notes: `.github/release.yml` groups them by label into Added (`feat`), Fixed (`fix`) and Performance (`perf`); every other label, an unlabelled pull request and Dependabot are left out.

1. After each merge to `main`, the Release Please workflow opens or updates a PR titled `chore(main): release X.Y.Z`. It bumps `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, and writes the new section of `CHANGELOG.md`. A follow-up job in the same workflow runs `cargo update --workspace` on that branch and pushes a `chore(main): update cargo.lock for the release` commit (only with the token below, so the checks run again).
2. Merge that PR (checks `ci-ok` and `pr-title` must pass, see the token note below). release-please creates the `vX.Y.Z` tag and a **draft** GitHub release, and the Release workflow builds the Windows installer (NSIS `-setup.exe`) into that draft.
3. Check the draft and its assets, then publish it.

**Token.** PRs and tags created with the default `GITHUB_TOKEN` do not trigger workflows, so the release PR would never get its checks. Create a fine-grained personal access token for this repository only (Repository permissions: Contents read and write, Pull requests read and write) and save it as the Actions secret `RELEASE_PLEASE_TOKEN`. Without the secret the workflow falls back to `GITHUB_TOKEN`: the release still builds, but the checks on the release PR stay pending until you push an empty commit to its branch yourself. Also enable Settings > Actions > General > "Allow GitHub Actions to create and approve pull requests".

**Update signing.** Installed copies update themselves from the published release, and the updater only accepts packages signed with our key. The public key is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`; the private key never enters the repository. Two Actions secrets are required:

- `TAURI_SIGNING_PRIVATE_KEY`: the key's content (from `pnpm tauri signer generate -w <file>`; a path does not work in CI). The Release workflow fails at once without it, instead of publishing an installer with no `latest.json`.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password (empty if none).

`tauri-action` then uploads the `.sig` files and `latest.json` to the draft release. Copies installed from older versions see the update only after you **publish** the draft, because the endpoint is `releases/latest/download/latest.json` and "latest" skips drafts. Keep the private key in a password manager: if it is lost, installed copies can never update again and users must reinstall by hand (a new key means a new `pubkey` that old copies reject). To test an update between two versions, see [docs/manual-tests/auto-update.md](docs/manual-tests/auto-update.md).

**Manual fallback** (release-please broken or unavailable):

1. `node scripts/bump-version.mjs X.Y.Z` (updates the four version files, `Cargo.lock` included).
2. In `CHANGELOG.md`, add a `## X.Y.Z (YYYY-MM-DD)` section (`## [X.Y.Z] - YYYY-MM-DD` also works); its text becomes the release notes.
3. Merge the change to `main`, then tag it: `git tag vX.Y.Z && git push origin vX.Y.Z`. The Release workflow builds and creates the draft; it fails if the tag differs from `tauri.conf.json`.
4. Also set the matching version in `.release-please-manifest.json` so release-please continues from it.
