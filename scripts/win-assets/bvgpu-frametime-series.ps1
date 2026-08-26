[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Id,
    [Parameter(Mandatory = $true)][string]$ProcessName,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [int]$WarmupSeconds = 30,
    [int]$MeasureSeconds = 60
)

# Generic per-process frame-time series for the 20-workload compatibility
# matrix. The earlier title instrument was written for one fixed executable
# and printed only summary scalars, so a matrix row could not carry the raw
# evidence the validator requires. This takes the process name as a parameter and writes
# "<id>.frametimes-ms": one positive frame interval per line, in presentation
# order, after the row's declared warmup.
#
# The host cannot measure this: RESOURCE_FLUSH is the desktop compositor's
# scanout, not the application's frames. Microsoft-Windows-DxgKrnl is captured
# with logman (in-box), decoded with tracerpt, and filtered to the target PID,
# so every interval is attributable to that process.
#
# Reports samples=0 with a reason rather than guessing. A refusal is a
# launch-fail/unsupported observation for that row; it is never a pass.
$ErrorActionPreference = 'SilentlyContinue'
function Emit($line) { Write-Output "BV-FT| $line" }
if (-not (Test-Path -LiteralPath $OutDir)) { [void](New-Item -ItemType Directory -Path $OutDir -Force) }
$series = Join-Path $OutDir "$Id.frametimes-ms"
Remove-Item -LiteralPath $series -Force -ErrorAction SilentlyContinue

$deadline = (Get-Date).AddSeconds($WarmupSeconds)
$proc = $null
while ((Get-Date) -lt $deadline) {
    $proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($proc) { break }
    Start-Sleep -Milliseconds 500
}
if (-not $proc) { Emit "id=$Id samples=0 reason=process-not-running"; Write-Output 'BV-FT-DONE'; exit }
$target = $proc.Id
Emit "id=$Id target_pid=$target warmup_s=$WarmupSeconds measure_s=$MeasureSeconds"
# Finish the declared warmup before the measurement window opens.
$remaining = ($deadline - (Get-Date)).TotalSeconds
if ($remaining -gt 0) { Start-Sleep -Seconds ([Math]::Ceiling($remaining)) }

$etl = Join-Path $OutDir "$Id.etl"
Remove-Item -LiteralPath $etl -Force -ErrorAction SilentlyContinue
logman stop bvft -ets 2>&1 | Out-Null
$start = logman start bvft -ets -p "Microsoft-Windows-DxgKrnl" 0xFFFF 0x5 -o $etl -ct perf -bs 1024 -nb 64 256 2>&1
Emit ("logman_start=" + ($start -join ' ').Trim())
Start-Sleep -Seconds $MeasureSeconds
logman stop bvft -ets 2>&1 | Out-Null
if (-not (Test-Path -LiteralPath $etl)) { Emit "id=$Id samples=0 reason=no-etl"; Write-Output 'BV-FT-DONE'; exit }

$csv = Join-Path $OutDir "$Id.csv"
Remove-Item -LiteralPath $csv -Force -ErrorAction SilentlyContinue
& tracerpt.exe $etl -o $csv -of CSV -y 2>&1 | Out-Null
if (-not (Test-Path -LiteralPath $csv)) { Emit "id=$Id samples=0 reason=tracerpt-failed"; Write-Output 'BV-FT-DONE'; exit }
$rows = Import-Csv $csv
if (-not $rows) { Emit "id=$Id samples=0 reason=empty-trace"; Write-Output 'BV-FT-DONE'; exit }
# tracerpt names columns with leading spaces; resolve by trimmed name.
$cols = $rows[0].PSObject.Properties.Name
$pidCol = $cols | Where-Object { $_.Trim() -eq 'PID' } | Select-Object -First 1
$taskCol = $cols | Where-Object { $_.Trim() -eq 'Task' } | Select-Object -First 1
$clockCol = $cols | Where-Object { $_.Trim() -eq 'Clock-Time' } | Select-Object -First 1
if (-not ($pidCol -and $taskCol -and $clockCol)) { Emit "id=$Id samples=0 reason=missing-columns"; Write-Output 'BV-FT-DONE'; exit }
# DxgKrnl task 178 = Present, 184 = PresentHistory; either marks one presented
# frame for this process.
$mine = @($rows | Where-Object { ($_.$pidCol -as [int]) -eq $target -and $_.$taskCol -match '^\s*(178|184)\s*$' })
Emit ("rows_total=" + $rows.Count + " present_events_target=" + $mine.Count)
$ticks = @($mine | ForEach-Object { $_.$clockCol -as [long] } | Where-Object { $_ -gt 0 } | Sort-Object)
if ($ticks.Count -lt 3) { Emit "id=$Id samples=0 reason=too-few-present-events"; Write-Output 'BV-FT-DONE'; exit }
# 100ns ticks -> milliseconds. Drop non-positive intervals: the validator
# requires every retained sample to be positive and finite.
$lines = New-Object System.Collections.Generic.List[string]
for ($i = 1; $i -lt $ticks.Count; $i++) {
    $ms = ($ticks[$i] - $ticks[$i - 1]) / 10000.0
    if ($ms -gt 0 -and $ms -le 60000) { $lines.Add($ms.ToString('0.###', [System.Globalization.CultureInfo]::InvariantCulture)) }
}
if ($lines.Count -lt 1) { Emit "id=$Id samples=0 reason=no-positive-intervals"; Write-Output 'BV-FT-DONE'; exit }
[System.IO.File]::WriteAllText($series, ($lines -join "`n") + "`n")
$sorted = $lines | ForEach-Object { [double]$_ } | Sort-Object
$q = { param($p) $sorted[[int][Math]::Floor(($sorted.Count - 1) * $p)] }
$sha = (Get-FileHash -LiteralPath $series -Algorithm SHA256).Hash.ToLower()
Emit ("id=$Id samples=" + $lines.Count + " p50_ms=" + (& $q 0.50) + " p95_ms=" + (& $q 0.95) +
      " p99_ms=" + (& $q 0.99) + " series_sha256=" + $sha)
Write-Output 'BV-FT-DONE'
