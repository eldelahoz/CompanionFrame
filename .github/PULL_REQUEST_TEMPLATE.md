## What and why

<!-- One concern per pull request. Link the issue if there is one. -->

## How it was tested

<!-- Tests added or changed, and anything you tried by hand. -->

## Checklist

- [ ] Commits follow `type(scope): summary`, 140 characters at most (see CONTRIBUTING.md)
- [ ] `pnpm build` and `pnpm test` pass
- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test` pass in `src-tauri/` (if Rust changed)
- [ ] `pnpm bindings` was run and `src/bindings.ts` is committed (if commands or IPC types changed)
- [ ] New text is in all five locale files (`en`, `es`, `fr`, `de`, `zh`)
- [ ] New database tables are a new entry at the end of `MIGRATIONS`
- [ ] No Digital Extremes art, trademarks, game screenshots, tokens or personal data are added
- [ ] Docs and manual-test checklists are updated if behavior changed
