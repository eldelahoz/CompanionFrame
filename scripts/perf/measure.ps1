#requires -Version 7
<#
.SYNOPSIS
  Samples CompanionFrame's process tree (and Warframe, for context) and writes a CSV plus a summary.

.DESCRIPTION
  Every -IntervalSeconds for -DurationMinutes it records, per group (app, webview2, tree, game):
  CPU % (of the whole machine), Working Set, Private Bytes, handles, threads, GDI/USER objects,
  IO read/write bytes/s and GPU engine utilisation (when the counter exists).
  The webview2 group is every msedgewebview2.exe descendant of the app process (parent PID tree).
  No admin, no installs. Output: <OutDir>\<scenario>-<timestamp>.csv and .summary.txt

.EXAMPLE
  pwsh scripts\perf\measure.ps1 -Scenario idle-game-running -DurationMinutes 10
.EXAMPLE
  pwsh scripts\perf\measure.ps1 -Scenario soak-1h -DurationMinutes 60 -IntervalSeconds 10
#>
[CmdletBinding()]
param(
    [ValidateSet('idle-no-game', 'idle-game-running', 'reward-scan', 'riven-cycling', 'move-mode', 'minimized', 'soak-1h')]
    [string]$Scenario = 'idle-no-game',
    [double]$DurationMinutes = 5,
    [int]$IntervalSeconds = 5,
    [string]$OutDir = (Join-Path $PSScriptRoot 'out'),
    # Process names without .exe. -AppName is the root of the tree; override only to test the script.
    [string]$AppName = 'companionframe',
    [string]$GameName = 'Warframe.x64',
    [string]$ChildName = 'msedgewebview2'
)

$ErrorActionPreference = 'Stop'
$cores = [Environment]::ProcessorCount

Add-Type -Namespace Perf -Name Native -MemberDefinition @'
[StructLayout(LayoutKind.Sequential)]
public struct IO_COUNTERS { public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes; }
[DllImport("kernel32.dll")] public static extern bool GetProcessIoCounters(IntPtr h, out IO_COUNTERS c);
[DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr h, uint flags);
[DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
[DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
'@

# PROCESS_QUERY_LIMITED_INFORMATION is enough for IO counters and GetGuiResources on same-user processes.
function Get-Native([int]$ProcessId) {
    $h = [Perf.Native]::OpenProcess(0x1000, $false, $ProcessId)
    if ($h -eq [IntPtr]::Zero) { return $null }
    try {
        $io = New-Object Perf.Native+IO_COUNTERS
        $ok = [Perf.Native]::GetProcessIoCounters($h, [ref]$io)
        [pscustomobject]@{
            Read  = if ($ok) { [double]$io.ReadBytes } else { 0.0 }
            Write = if ($ok) { [double]$io.WriteBytes } else { 0.0 }
            Gdi   = [Perf.Native]::GetGuiResources($h, 0)
            User  = [Perf.Native]::GetGuiResources($h, 1)
        }
    } finally { [void][Perf.Native]::CloseHandle($h) }
}

# GPU engine counters are optional (older drivers, some VMs).
$gpuOk = $false
try { [void](Get-Counter '\GPU Engine(*)\Utilization Percentage' -ErrorAction Stop); $gpuOk = $true }
catch { Write-Warning 'GPU Engine counters unavailable; gpu_pct will be empty.' }

function Get-GpuByPid {
    $map = @{}
    if (-not $gpuOk) { return $map }
    try {
        foreach ($s in (Get-Counter '\GPU Engine(*)\Utilization Percentage' -ErrorAction Stop).CounterSamples) {
            if ($s.InstanceName -match 'pid_(\d+)_') {
                $p = [int]$Matches[1]
                $map[$p] = ($map[$p] ?? 0.0) + $s.CookedValue
            }
        }
    } catch { }
    $map
}

# Every msedgewebview2 descendant of the app (a child's own children included).
function Get-Pids {
    $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name
    $app  = @($all | Where-Object { $_.Name -ieq "$AppName.exe" }).ProcessId
    $game = @($all | Where-Object { $_.Name -ieq "$GameName.exe" }).ProcessId
    $kids = [System.Collections.Generic.HashSet[int]]::new()
    $queue = [System.Collections.Generic.Queue[int]]::new()
    foreach ($a in $app) { $queue.Enqueue([int]$a) }
    while ($queue.Count) {
        $parent = $queue.Dequeue()
        foreach ($c in $all | Where-Object { $_.ParentProcessId -eq $parent }) {
            if ($kids.Add([int]$c.ProcessId) -and $c.Name -ieq "$ChildName.exe") { $queue.Enqueue([int]$c.ProcessId) }
        }
    }
    # Keep only webview2 hosts; other children of the app (a helper exe) are not part of the tree budget.
    $web = @($all | Where-Object { $kids.Contains([int]$_.ProcessId) -and $_.Name -ieq "$ChildName.exe" }).ProcessId
    @{ app = [int[]]$app; webview2 = [int[]]$web; game = [int[]]$game }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$csv = Join-Path $OutDir "$Scenario-$stamp.csv"
$summaryPath = Join-Path $OutDir "$Scenario-$stamp.summary.txt"

$prev = @{}   # pid -> @{ Cpu = seconds; Read; Write; At }
$rows = [System.Collections.Generic.List[object]]::new()
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$end = $DurationMinutes * 60
Write-Host "Scenario '$Scenario' for $DurationMinutes min, every ${IntervalSeconds}s, $cores logical cores -> $csv"

do {
    $tick = $sw.Elapsed.TotalSeconds
    $groups = Get-Pids
    $gpu = Get-GpuByPid
    $now = Get-Date
    $acc = @{}
    foreach ($g in 'app', 'webview2', 'game') {
        $acc[$g] = [ordered]@{ procs = 0; cpu = 0.0; ws = 0.0; priv = 0.0; handles = 0; threads = 0; gdi = 0; user = 0; rd = 0.0; wr = 0.0; gpu = 0.0 }
    }
    foreach ($g in 'app', 'webview2', 'game') {
        foreach ($procId in $groups[$g]) {
            $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
            if (-not $p) { continue }
            $n = Get-Native $procId
            $a = $acc[$g]
            $a.procs++
            $a.ws += $p.WorkingSet64 / 1MB
            $a.priv += $p.PrivateMemorySize64 / 1MB
            $a.handles += $p.HandleCount
            $a.threads += $p.Threads.Count
            if ($gpu.ContainsKey($procId)) { $a.gpu += $gpu[$procId] }
            if ($n) { $a.gdi += $n.Gdi; $a.user += $n.User }
            $cpu = $p.TotalProcessorTime.TotalSeconds
            $before = $prev[$procId]
            if ($before) {
                $dt = ($now - $before.At).TotalSeconds
                if ($dt -gt 0) {
                    $a.cpu += ($cpu - $before.Cpu) / $dt / $cores * 100
                    if ($n) {
                        $a.rd += [math]::Max(0, ($n.Read - $before.Read) / $dt)
                        $a.wr += [math]::Max(0, ($n.Write - $before.Write) / $dt)
                    }
                }
            }
            $prev[$procId] = @{ Cpu = $cpu; Read = if ($n) { $n.Read } else { 0 }; Write = if ($n) { $n.Write } else { 0 }; At = $now }
        }
    }
    # The first sample per group has no delta; it is dropped after the loop so CPU/IO are never a fake zero.
    & {
        $tree = [ordered]@{}
        foreach ($k in $acc['app'].Keys) { $tree[$k] = $acc['app'][$k] + $acc['webview2'][$k] }
        $set = [ordered]@{ app = $acc['app']; webview2 = $acc['webview2']; tree = $tree; game = $acc['game'] }
        foreach ($g in $set.Keys) {
            $a = $set[$g]
            if ($a.procs -eq 0) { continue }
            $rows.Add([pscustomobject][ordered]@{
                    time = $now.ToString('o'); elapsed_s = [math]::Round($tick, 1); scenario = $Scenario; group = $g
                    procs = $a.procs; cpu_pct = [math]::Round($a.cpu, 3); ws_mb = [math]::Round($a.ws, 1); private_mb = [math]::Round($a.priv, 1)
                    handles = $a.handles; threads = $a.threads; gdi = $a.gdi; user = $a.user
                    io_read_bps = [math]::Round($a.rd); io_write_bps = [math]::Round($a.wr)
                    gpu_pct = if ($gpuOk) { [math]::Round($a.gpu, 2) } else { '' }
                })
        }
    }
    # Sampling itself takes a second or two (CIM + GPU counters), so sleep to the next slot, not a fixed gap.
    $nextAt = $tick + $IntervalSeconds
    if ($nextAt -ge $end) { break }
    $wait = $nextAt - $sw.Elapsed.TotalSeconds
    if ($wait -gt 0) { Start-Sleep -Milliseconds ([int](1000 * $wait)) }
} while ($true)

# The first sample per group only seeds the deltas; drop it from the CSV.
$firstSeen = @{}
$kept = foreach ($r in $rows) { if ($firstSeen.ContainsKey($r.group)) { $r } else { $firstSeen[$r.group] = $true } }
$kept | Export-Csv -Path $csv -NoTypeInformation -Encoding utf8

function Get-Slope($xs, $ys) {
    # Least squares, per hour. xs in seconds.
    $n = $xs.Count
    if ($n -lt 4) { return $null }
    $mx = ($xs | Measure-Object -Average).Average; $my = ($ys | Measure-Object -Average).Average
    $num = 0.0; $den = 0.0
    for ($i = 0; $i -lt $n; $i++) { $num += ($xs[$i] - $mx) * ($ys[$i] - $my); $den += [math]::Pow($xs[$i] - $mx, 2) }
    if ($den -eq 0) { return 0.0 }
    $num / $den * 3600
}
function Get-P95($v) { $s = @($v | Sort-Object); $s[[math]::Min($s.Count - 1, [math]::Ceiling($s.Count * 0.95) - 1)] }

# Flag thresholds are the soak budget in docs/perf/budgets.md.
$leakLimit = @{ private_mb = 10.0; ws_mb = 20.0; handles = 100.0; threads = 5.0; gdi = 20.0; user = 20.0 }
$metrics = 'cpu_pct', 'ws_mb', 'private_mb', 'handles', 'threads', 'gdi', 'user', 'io_read_bps', 'io_write_bps', 'gpu_pct'
$out = [System.Collections.Generic.List[string]]::new()
$out.Add("scenario=$Scenario  samples=$(@($kept | Where-Object group -eq 'tree').Count)  interval=${IntervalSeconds}s  duration=$([math]::Round($sw.Elapsed.TotalMinutes,1))min  cores=$cores  gpu_counters=$gpuOk")
$out.Add("host=$env:COMPUTERNAME  os=$([Environment]::OSVersion.VersionString)  pwsh=$($PSVersionTable.PSVersion)")
$flags = [System.Collections.Generic.List[string]]::new()
foreach ($g in 'app', 'webview2', 'tree', 'game') {
    $r = @($kept | Where-Object group -eq $g)
    if (-not $r.Count) { $out.Add(''); $out.Add("[$g] no process found"); continue }
    $out.Add(''); $out.Add("[$g]  (max procs seen: $(($r | Measure-Object procs -Maximum).Maximum))")
    $out.Add(('{0,-14}{1,12}{2,12}{3,12}{4,16}' -f 'metric', 'avg', 'p95', 'max', 'slope/hour'))
    foreach ($m in $metrics) {
        $v = @($r | ForEach-Object { if ($_.$m -isnot [string]) { [double]$_.$m } })
        if (-not $v.Count) { continue }
        $avg = ($v | Measure-Object -Average).Average; $max = ($v | Measure-Object -Maximum).Maximum
        $slopeText = ''
        if ($leakLimit.ContainsKey($m)) {
            # Skip the first 10% as warm-up so start-up allocation is not read as a leak.
            $skip = [int]($v.Count * 0.1)
            $xs = @($r | Select-Object -Skip $skip | ForEach-Object { [double]$_.elapsed_s })
            $slope = Get-Slope $xs @($v | Select-Object -Skip $skip)
            if ($null -ne $slope) {
                $slopeText = '{0:N1}' -f $slope
                if ($g -in 'app', 'webview2', 'tree' -and $slope -gt $leakLimit[$m] -and $sw.Elapsed.TotalMinutes -ge 10) {
                    $flags.Add("LEAK? [$g] $m grows ~$slopeText/hour (limit $($leakLimit[$m]))")
                }
            }
        }
        $out.Add(('{0,-14}{1,12:N2}{2,12:N2}{3,12:N2}{4,16}' -f $m, $avg, (Get-P95 $v), $max, $slopeText))
    }
}
$out.Add('')
$out.Add($(if ($flags.Count) { $flags } elseif ($sw.Elapsed.TotalMinutes -lt 10) { 'Leak check needs a run of 10 min or more; slopes above are indicative only.' } else { 'No growth flags.' }))
$out | Set-Content -Path $summaryPath -Encoding utf8
$out | ForEach-Object { Write-Host $_ }
Write-Host "`nCSV:     $csv`nSummary: $summaryPath"
