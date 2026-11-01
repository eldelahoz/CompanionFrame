# Resource budgets (proposed)

Warframe is the priority process. These are *proposed* limits, set from the static estimates, not from measurements. Phase 2 measures a release build against them and moves a number
only with data. "Machine %" is `cpu_pct` from `measure.ps1` (100 = every logical core). On a 16-core machine one
core is 6.25%, so "1% of one core" is 0.0625% machine; on 8 cores it is 0.125%. The tables use **% of one core**
as the unit and the script's `cpu_pct` x cores / 100 converts to it.

All figures are for the whole tree: `companionframe.exe` + every `msedgewebview2.exe` child (`group=tree`).

## Per scenario

| Scenario | CPU avg (tree) | CPU p95 | Working set (tree) | Private bytes (tree) | Disk writes | GPU |
|----------|----------------|---------|--------------------|----------------------|-------------|-----|
| `idle-no-game` | < 1% of one core | < 3% | < 600 MB | < 500 MB | < 100 KB/min | < 1% |
| `minimized` | < 0.5% of one core | < 2% | < 600 MB | < 500 MB | < 100 KB/min | 0 (no rendering) |
| `idle-game-running` | < 1.5% of one core | < 4% | < 700 MB | < 600 MB | < 200 KB/min | < 1% |
| `reward-scan` (per scan, 15 s window) | peak < 150% of one core for < 3 s, then back to idle within 10 s | | < 1.0 GB with models warm | < 900 MB | < 5 MB per scan (0 unless "save scan captures" is on) | < 3% |
| `riven-cycling` (per roll) | peak < 150% of one core for < 3 s | | as reward-scan | | < 5 MB per roll | < 3% |
| `move-mode` (while on) | < 8% of one core (four 250 ms measure timers, 50 ms cursor poll) | < 15% | idle + 20 MB | idle + 20 MB | 0 except layout saves (< 10 KB) | < 3% |
| `soak-1h` | as `idle-game-running` | | growth limits below | growth limits below | < 200 KB/min | < 1% |

Why these numbers:

* **Idle < 1% of one core.** A game companion should not move the CPU needle or the fan. Roughly 11 wakeups/s
  (audit section 1) should fit in 0.3-0.5%; 1% leaves room for WebView2's own housekeeping.
* **Scan bursts.** OCR + the line detector use up to 4 intra-op threads each (`session.rs:46`), so the burst is a few cores for
  a couple of seconds. That is acceptable if short; what matters is that it ends and the app returns to idle.
* **Memory.** The tree RSS ceiling is dominated by WebView2: 1 main + 4 hidden overlay renderers + browser/GPU/utility
  processes. 600 MB idle is a ceiling to fight for, not a measured fact: if measurements show the five renderers alone
  are over it, the number moves up or R10 (lazy overlays) gets done. With the ONNX models warm the budget is 1.0 GB.
* **Disk writes.** Idle writes should be near zero: logs, a state file on change, SQLite on events. 100 KB/min is
  6 MB/h, far below what an SSD notices, but sustained polling writes (for example the image cache's mtime touches)
  would show up here.
* **GPU.** Hidden overlays and a minimized main window should not render. Anything above 1% idle means a page is
  animating or compositing unnecessarily (check CSS animations, `backdrop-filter`, canvas loops).

## Soak (1 h) growth limits

Computed by the script as a least-squares slope per hour after discarding the first 10% as warm-up. These are the
limits `measure.ps1` flags (`$leakLimit`), applied to `app`, `webview2` and `tree` (the script asks for at least 10 minutes of data):

| Metric | Limit per hour |
|--------|----------------|
| Private bytes | +10 MB |
| Working set | +20 MB |
| Handles | +100 |
| Threads | +5 |
| GDI objects | +20 |
| USER objects | +20 |

A genuine fix is "flat": a slope close to 0 once models and caches are warm. A small positive slope that levels off
(a cache filling to its cap) is acceptable if the cap is documented: the image cache is 96 MB on disk and 0 in RAM; the
EE.log broadcast is 4096 lines.

## Latency and responsiveness

| Item | Target | How to measure |
|------|--------|----------------|
| Keyboard hook callback, non-bound key | p99 < 0.1 ms, max < 1 ms | to do: unit benchmark of `Matcher::on_key` + the real `hook_proc` timed with `QueryPerformanceCounter`, log p99/max in a debug build |
| Keyboard hook callback, bound key | p99 < 1 ms, max < 5 ms (Windows removes a hook that exceeds `LowLevelHooksTimeout`, 300 ms by default; stay two orders of magnitude below) | same; today this includes a process snapshot (R1) |
| Hotkey press to overlay visible | < 150 ms (excluding OCR) | manual, screen capture at 60 fps, count frames |
| Reward scan, key press to prices shown | < 2.5 s (the code's own price wait is 2.5 s, `reward.rs:41`) | log timestamps |
| Game frame time impact | p99 frame time with app open within 2% of app closed; no new stutter > 1 frame in a 10 min run | PresentMon / in-game, same scene, both ways |
| App start to tray ready | < 3 s | log timestamps |

## Disk and logs

| Item | Budget |
|------|--------|
| Log volume | < 5 MB/day at the default level, < 50 MB total retained (7 daily files) |
| Log level in release | `info` (R8) |
| Image cache | 96 MB cap (exists); eviction work < 5 ms per put (R6) |
| Scan captures | newest 20 (exists), opt-in |
| SQLite | main database < 100 MB after a year; WAL file < 4 MB after a checkpoint; a write transaction < 20 ms |
| Writes at idle | see the table above |

## Process counts

| Scenario | Limit |
|----------|-------|
| `companionframe.exe` threads | < 60 at idle, < 90 during a scan (tokio workers + the 4 named threads + the ONNX pools) |
| WebView2 processes | <= 9 with only the main window open (browser, GPU, network utility, crashpad, 1 main + 4 overlay renderers) |
| Handles (tree) | < 4,000 at idle |

If the first measurement shows a limit is wrong, the budget changes in this file with the data attached, not the code
quietly adapting. Budgets that cannot be measured with the harness (hook latency, frame time) are marked as such.
