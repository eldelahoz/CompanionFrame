# F6-T4 – ONNX Runtime (manual)

Does not need the game. Needs a machine with the app **installed** (or `pnpm tauri build`), ideally clean (no extra Rust/VC++).

1. Run `pnpm fetch-onnx` and `pnpm tauri build`; install the generated `.exe`. Note the installer size (estimated: +26 MB).
2. In the install folder check `onnxruntime/onnxruntime.dll` and `riven-ocr/paddle/…`.
3. Open the app and, in the webview console, run `await window.__TAURI_INTERNALS__.invoke('ocr_onnx_status')`. Expect
   `ready: true`, `runtimePresent/paddlePresent/detectorPresent: true`, `error: null`.
4. Rename `onnxruntime/onnxruntime.dll` and restart: `ready: false`, `error` mentions "onnxruntime.dll not found", and the app does not crash.
5. On a machine with an old `onnxruntime.dll` in `C:\Windows\System32` (recent Windows ships one): step 3 still gives `ready: true` (the DLL is loaded by absolute path).
