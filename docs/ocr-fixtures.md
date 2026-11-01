# Reward OCR fixtures: results and how to run

Harness: `src-tauri/examples/ocr_fixtures.rs`. From `src-tauri/`:

- `cargo run --example ocr_fixtures -- <dir>` runs the pipeline on a folder holding `manifest.json` and the saved reward-screen PNGs. The 14-slot frame set behind the results below came from the original WFHelper project's reward-scan fixtures (MIT) and is not shipped in this repository, because the frames are game screenshots.
- `cargo run --release --example ocr_fixtures -- --scan` is the F6-T6 scan check (`docs/manual-tests/f6-t6-reward-scanner.md`).
- `cargo run --example ocr_fixtures -- --screen` OCRs the primary screen after 5 s (game in front).

Pipeline (first prototype): fixed 2-4 card title rects (aspect-corrected) -> 3 bands (top 58 %, bottom 58 %, whole) -> resize to 720 px,
normalise 1-99 %, Otsu, dark-on-white -> Windows OCR (en-US) -> drop 1-char tokens. Per slot the better of {top+bottom joined, whole} is kept.
Pass = Levenshtein ratio >= 0.86 (the fuzzy gate) against the expected name; quantity prefixes ("2 X") are stripped from the expectation.

## First prototype: 11/14 pass the gate, 8/14 exact

| fixture | slot | expected | got | sim | |
|---|---|---|---|---|---|
| real-full-2p | 0 | Khora Prime Systems Blueprint | Khora Prinhe Systems Blueprint | 0.93 | ok |
| real-full-2p | 1 | Fang Prime Handle | Fang Prime Handle | 1.00 | ok |
| real-full-4p-fps | 0 | Xaku Prime Chassis Blueprint | assis Blueprint________..... | 0.54 | MISS |
| real-full-4p-fps | 1 | Bronco Prime Receiver | Bronco Prime Receiver | 1.00 | ok |
| real-full-4p-fps | 2 | Paris Prime Lower Limb | Paris Prime Lower Limb | 1.00 | ok |
| real-full-4p-fps | 3 | Vadarya Prime Barrel | vadarva Prime Barrel | 0.95 | ok |
| real-full-4p-16x10 | 0-2 | 2 X Forma Blueprint | Forma Blueprint | 1.00 | ok |
| real-full-4p-16x10 | 3 | 2 X Forma Blueprint | Fotma Blueprint | 0.93 | ok |
| real-full-4p-1080x607 | 0 | Okina Prime Handle | Okina Prime Handle | 1.00 | ok |
| real-full-4p-1080x607 | 1 | Velox Prime Barrel | Velox Prime Barrel | 1.00 | ok |
| real-full-4p-1080x607 | 2 | Caliban Prime Blueprint | Calibåh Prin;e Blueorint | 0.83 | MISS |
| real-full-4p-1080x607 | 3 | Grendel Prime Blueprint | Grendel PrinÆBlubrin | 0.74 | MISS |

## F6-T6 (`services/reward_scan`): 12/14 pass the gate, 9/14 exact

Whole frames through the real pipeline: card counter (bars, then Legacy/custom UI-scale sweep, then activity guess) -> title strips -> bands ->
`join_lines` -> Windows OCR. Card count right on 4/4 frames. Release, per frame (counter + 3 OCR reads per slot): 2p 65 ms, 4p 50-110 ms.

- Xaku Prime Chassis Blueprint passes: plain Otsu fuses the card art into "Prime", so the top band reads "assis". A strict cut (Otsu again over the pixels
  above the first threshold) reads "Xaku Prime Chassis" and is joined to the Otsu bottom band ("Blueprint"). The strict cut on the whole strip loses the dimmer second line.
- Caliban and Grendel at 1080x607 (small text) stay missing without ONNX.
- 1080x607 has no countable bar (under a pixel tall); the activity guess gives 4 cards.

## F6-T7 (matcher): 13/14 resolve to the right item

Each slot's candidate texts go through `reward_scan::slots::match_slot` against the 608 reward items of the bundled `Relics.json`. Caliban at 1080x607 is rescued by the token aliases.

## F6-T8 (ONNX reader, `reward_scan::onnx_reader`): 14/14 resolve to the right item

`OnnxReader` wraps the Windows reader and adds one candidate per slot: PaddleOCR over the raw RGB rows of the strip. Both reads always run and the matcher ranks them.
Release, whole frame incl. Windows reads: 2p 85 ms, 4p 90-220 ms. Grendel Prime Blueprint at 1080x607 is recovered by ONNX. Without the DLL/models the pipeline is the T6 one
(13/14 through the matcher); the example prints both columns.

## Limits

- Capture is the primary screen or a window client area via GDI BitBlt; borderless/exclusive-fullscreen behaviour over the live game is untested.

## Latin recognizer (PP-OCRv5 Latin, RapidOCR v3.4.0 export): CH v3 vs Latin

`OCR_MODEL=latin cargo run --release --example ocr_fixtures -- <fixtures>` scores the Latin model; the default is CH. The "onnx alone" column runs Paddle
with no Windows reads.

| model | with Windows OCR | ONNX alone (matched) | ONNX alone (raw text exact) | misses (raw) |
|---|---|---|---|---|
| CH PP-OCRv3 (10.7 MB) | 14/14 | 14/14 | 11/14 | Foima, Caliban Prme Bluerint, Grende Prme |
| Latin PP-OCRv5 mobile (7.9 MB) | 14/14 | 14/14 | 13/14 | Foima |

English does not drop, so the Latin model reads every Latin-script game language including English (`PaddleModel::for_language`);
CH stays for zh/tc/ja/ko/ru/uk/th. The Latin model keeps accents (`tests/fixtures/ocr_onnx/strip_fr.png` reads "Volt Prime Châssis Schéma";
CH reads "Chassis"). The riven scan tests pass with both models. The Latin model can drop the last letter of an untrimmed full strip, so it
relies on the row crops the pipeline already makes.

## Paddle primary (live order since fixes2)

`overlay::reward` reads each frame with Paddle alone first; Windows OCR (plus Paddle again) runs only for slots Paddle left
unread (`reward_scan::flow::scan_once`). No Windows language pack is needed. The harness prints this as "Paddle primary".

| pipeline (Latin PP-OCRv5) | matched |
|---|---|
| Windows OCR alone | 13/14 (Grendel Prime Blueprint at 1080x607 misses) |
| Paddle alone | 14/14 |
| Paddle primary + Windows fallback | 14/14 |
