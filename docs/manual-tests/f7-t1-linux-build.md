# F7-T1 – Building on Linux

## System prerequisites (need sudo; one line)

```sh
sudo apt-get update && sudo apt-get install -y --no-install-recommends libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libasound2-dev libssl-dev libxdo-dev patchelf build-essential curl
```

Without passwordless sudo (as on the WSL Debian used for the first check), extract those `.deb` files without root into `~/sysroot/root` and build with
`PKG_CONFIG_SYSROOT_DIR`, `PKG_CONFIG_PATH`, `RUSTFLAGS=-L ...` and `LD_LIBRARY_PATH` pointing there, and `CARGO_TARGET_DIR` on the Linux filesystem.

## What was checked (WSL Debian, rustc 1.97.1)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` (1216 passed) were green.
- `cargo run --example export_bindings` produces the same `src/bindings.ts` as on Windows (apart from CRLF).
- Tests that load `onnxruntime.dll` (`ocr_onnx::loads_bundled_dll…`, `riven_scan::scan_tests` with models) are `cfg(windows)` until the Linux ONNX runtime is bundled.
- vitest and `pnpm build` were not run in WSL (no pnpm there); they do not depend on the OS.

## Open

- `ws::tests::three_signin_rejections_give_up_until_a_new_token` failed once under load with tests running in parallel and passed 4 more times (possible timing flake).
