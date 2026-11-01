# How to measure CompanionFrame's resource use

You run `scripts/perf/measure.ps1` next to a real game session and attach the CSV files to your report. No admin rights and no
installs. It needs PowerShell 7 (`pwsh`). It reads process counters only; it does not touch the app or the game.

## What it records

Every N seconds, for the app (`companionframe.exe`), its WebView2 children (`msedgewebview2.exe`, matched through the
parent-process tree), the two together (`tree`) and Warframe (`game`, context only):

CPU % of the whole machine, Working Set, Private Bytes, handles, threads, GDI and USER objects, disk read/write
bytes per second, GPU engine utilisation (if Windows exposes the counter).

It writes `scripts/perf/out/<scenario>-<timestamp>.csv` and a `.summary.txt` next to it (avg / p95 / max per metric
and a growth slope per hour for memory, handles and threads).

## One-time setup

1. Use a **release build** of the app (`companionframe.exe` from `src-tauri/target/release`, or the installed app). A
   debug build is several times heavier and will not match the budgets.
2. Open PowerShell 7 in the repo root: `pwsh`.
3. If scripts are blocked: `pwsh -ExecutionPolicy Bypass -File scripts\perf\measure.ps1 ...` (no system change).
4. Close other heavy apps (browsers, video, launchers) so they do not pollute the machine-wide numbers. The script
   measures only the app, its WebView2 processes and Warframe, but the game itself runs differently on a busy PC.

## The scenarios

Always start the app fresh (quit it from the tray first) and let it sit for 2 minutes before you start a run, so
start-up work is not counted. One scenario per run. The label only names the file; do the action yourself.

| Scenario | Game | What you do while it runs | Duration |
|----------|------|---------------------------|----------|
| `idle-no-game` | closed | Main window open on the dashboard. Do nothing. | 10 min |
| `minimized` | closed | Minimize the main window to the taskbar. Do nothing. | 10 min |
| `idle-game-running` | open, in the orbiter or a quiet spot | Leave the app in the background (game in front). Do nothing. | 10 min |
| `reward-scan` | open | Run 3 fissure missions in a row; let the reward scan fire each time. Note the clock time of each. | until done, about 20-30 min |
| `riven-cycling` | open | Open the riven cycle screen and roll 10 times with the riven overlay on. | until done |
| `move-mode` | open | Turn move mode on (the move key), drag two panels around for 2 minutes, turn it off, and keep the run going another 3 minutes. | 5-7 min |
| `soak-1h` | open (a normal play session) | Play normally with the app running, main window minimized. Do not touch the app. | 60 min |

## Running it

```powershell
# 10 minutes, one sample every 5 s (the default interval)
pwsh scripts\perf\measure.ps1 -Scenario idle-game-running -DurationMinutes 10

# the 1 h soak, one sample every 10 s
pwsh scripts\perf\measure.ps1 -Scenario soak-1h -DurationMinutes 60 -IntervalSeconds 10

# a dry run on any process, to check the script works on your machine (10 s)
pwsh scripts\perf\measure.ps1 -Scenario idle-no-game -DurationMinutes 0.17 -IntervalSeconds 2 -AppName explorer
```

Start the script, **then** do the scenario's action. Each sample takes 1-3 seconds of its own (process list and GPU
counters); the script sleeps to the next slot, so the real interval is whatever you asked for as long as it is 5 s
or more.

At the end it prints the summary. Read the bottom lines:

* `LEAK? ...` means a growth slope above the soak limit (needs a run of 10 minutes or more).
* `no process found` for `[app]` means the app was not running (or is named differently): check `-AppName`.
* `gpu_pct` empty or 0 everywhere on a system with a GPU means the counter is missing; say so in your message.

## Sending the results back

1. From `scripts\perf\out\`, take the `.csv` **and** the `.summary.txt` of every run.
2. Zip them (`Compress-Archive scripts\perf\out\* perf-results.zip`) and send the zip.
3. With the zip, say: the app version/commit, game on which graphics settings and resolution, CPU and GPU model,
   whether you were on battery, and anything unusual that happened (a crash, a long loading screen, a Windows update).

The CSVs contain only process names, PIDs' aggregates and numbers: no paths, account names or game data.

## Reading a CSV yourself

One row per sample per group. Columns: `time, elapsed_s, scenario, group, procs, cpu_pct, ws_mb, private_mb,
handles, threads, gdi, user, io_read_bps, io_write_bps, gpu_pct`. `group` is `app`, `webview2`, `tree` or `game`.
`cpu_pct` is of the whole machine (100 = every logical core busy), so on 16 cores one fully used core is 6.25.
`webview2` also covers the four overlay windows' renderers (they are created hidden at start-up).

## Not covered by this script

* Hook latency and the game's frame time. For frame time use PresentMon or the game's own FPS counter with the app
  open and then closed, same scene.
* A per-WebView2-process breakdown. In Task Manager > Details, add the "Command line" column to tell the main
  window, the overlays and the GPU process apart.
* Log size per day: check `%APPDATA%\<app id>\logs` (the app log dir) before and after a day of use and send the
  file sizes.
