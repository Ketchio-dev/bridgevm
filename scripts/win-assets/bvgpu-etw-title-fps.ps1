# Guest-measured, process-attributed frame rate for the real-title gates.
#
# The host cannot measure a title's frame rate: RESOURCE_FLUSH is issued by the
# desktop compositor on its own context, so a host-side flush rate counts DWM's
# scanout, not the application's frames. PPSSPP's own log has no machine-readable
# FPS line either. This captures Microsoft-Windows-DxgKrnl with logman (built
# into Windows), decodes it with tracerpt, and keeps only rows whose PID matches
# the title, so every number is attributable to that process.
#
# Reports samples=0 with a reason rather than guessing.
$ErrorActionPreference='SilentlyContinue'
$app='C:\BVPSPAPP'; $share='C:\BVPSP'
$p = Get-Process PPSSPPWindowsARM64
if (-not $p) { Write-Output 'BV-FPS| status=FAIL reason=title-not-running'; Write-Output 'BV-FPS-DONE'; exit }
$target = $p.Id
Write-Output ("BV-FPS| target_pid=$target window=" + $p.MainWindowTitle)
$mods = @($p.Modules | Where-Object { $_.ModuleName -match 'vulkan_virtio' } | ForEach-Object { $_.ModuleName })
Write-Output ("BV-FPS| graphics_path=" + ($mods -join ','))
$etl = Join-Path $share 'fps.etl'
Remove-Item $etl -Force -ErrorAction SilentlyContinue
logman stop bvfps -ets 2>&1 | Out-Null
$dur = 30
# 0xFFFF enables the Present/PresentHistory keywords too; the earlier 0x1 mask
# only produced ~1 event/s per task, which cannot be per-frame.
$r = logman start bvfps -ets -p "Microsoft-Windows-DxgKrnl" 0xFFFF 0x5 -o $etl -ct perf -bs 1024 -nb 64 256 2>&1
Write-Output ("BV-FPS| logman_start=" + ($r -join ' ').Trim())
Start-Sleep -Seconds $dur
logman stop bvfps -ets 2>&1 | Out-Null
if (-not (Test-Path $etl)) { Write-Output 'BV-FPS| status=FAIL reason=no-etl'; Write-Output 'BV-FPS-DONE'; exit }
Write-Output ("BV-FPS| etl_bytes=" + (Get-Item $etl).Length + " window_s=$dur")
$csv = Join-Path $share 'fps.csv'
Remove-Item $csv -Force -ErrorAction SilentlyContinue
& tracerpt.exe $etl -o $csv -of CSV -y 2>&1 | Out-Null
if (-not (Test-Path $csv)) { Write-Output 'BV-FPS| status=FAIL reason=tracerpt-failed'; Write-Output 'BV-FPS-DONE'; exit }
Write-Output ("BV-FPS| csv_bytes=" + (Get-Item $csv).Length)
$hdr = Get-Content $csv -TotalCount 1
Write-Output ("BV-FPS| csv_header=" + $hdr)
$rows = Import-Csv $csv
# tracerpt names the columns with leading spaces; find the PID/Task columns by
# trimmed name rather than assuming.
$cols = ($rows[0].PSObject.Properties.Name)
$pidCol  = $cols | Where-Object { $_.Trim() -eq 'PID' } | Select-Object -First 1
$taskCol = $cols | Where-Object { $_.Trim() -eq 'Task' } | Select-Object -First 1
$clockCol= $cols | Where-Object { $_.Trim() -eq 'Clock-Time' } | Select-Object -First 1
Write-Output ("BV-FPS| pid_col='" + $pidCol + "' task_col='" + $taskCol + "'")
$mine = @($rows | Where-Object { ($_.$pidCol -as [int]) -eq $target })
Write-Output ("BV-FPS| rows_total=" + $rows.Count + " rows_target=" + $mine.Count)
$byTask = $mine | Group-Object $taskCol | Sort-Object Count -Descending | Select-Object -First 12
foreach ($t in $byTask) { Write-Output ("BV-FPS-TASK| task=" + $t.Name.Trim() + " n=" + $t.Count + " per_s=" + [Math]::Round($t.Count/$dur,2)) }
# Present submissions for this process. DxgKrnl task 178 = Present, 184 =
# PresentHistory; accept either, and report which was used.
$opCol = $cols | Where-Object { $_.Trim() -eq 'Opcode' } | Select-Object -First 1
$idCol = $cols | Where-Object { $_.Trim() -eq 'Event ID' } | Select-Object -First 1
# Task 68 dominates; break it down by opcode/event id so the per-frame marker is
# identifiable instead of guessed.
$t68 = @($mine | Where-Object { $_.$taskCol.Trim() -eq '68' })
$t68 | Group-Object $opCol | Sort-Object Count -Descending | Select-Object -First 6 | ForEach-Object {
  Write-Output ("BV-FPS-OP68| opcode=" + $_.Name.Trim() + " n=" + $_.Count + " per_s=" + [Math]::Round($_.Count/$dur,2)) }
$mine | Group-Object $idCol | Sort-Object Count -Descending | Select-Object -First 10 | ForEach-Object {
  Write-Output ("BV-FPS-EID| id=" + $_.Name.Trim() + " n=" + $_.Count + " per_s=" + [Math]::Round($_.Count/$dur,2)) }
# VSync/Present candidates land in the 30-70/s band for a 60 Hz title.
$pres = @($mine | Where-Object { $_.$taskCol -match '^\s*(178|184)\s*$' })
Write-Output ("BV-FPS| present_events_target=" + $pres.Count)
if ($pres.Count -gt 0) {
  $ticks = @($pres | ForEach-Object { $_.$clockCol -as [long] } | Where-Object { $_ -gt 0 } | Sort-Object)
  if ($ticks.Count -gt 2) {
    $span = ($ticks[-1] - $ticks[0]) / 1e7
    $fps = [Math]::Round(($ticks.Count - 1) / $span, 2)
    $deltas = @(); for ($i=1; $i -lt $ticks.Count; $i++) { $deltas += ($ticks[$i]-$ticks[$i-1]) / 1e7 }
    $sd = $deltas | Sort-Object
    $p50dt = $sd[[int][Math]::Floor(($sd.Count-1)*0.5)]
    $p50 = if ($p50dt -gt 0) { [Math]::Round(1.0/$p50dt,2) } else { 0 }
    Write-Output ("BV-FPS| samples=" + $ticks.Count + " span_s=" + [Math]::Round($span,2) + " fps_mean=" + $fps + " p50=" + $p50)
  } else {
    Write-Output ("BV-FPS| samples=" + $pres.Count + " reason=too-few-timestamps")
  }
} else {
  Write-Output 'BV-FPS| samples=0 reason=no-present-events-for-target'
}
Write-Output 'BV-FPS-DONE'
