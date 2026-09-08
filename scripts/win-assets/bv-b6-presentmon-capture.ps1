# Diagnostic-only guest asset for the B6 glyph matrix's frame-time leg
# (declared in capabilities/windows-hvf.json B6: "frame time within 10% of
# baseline"). Not part of any shipped closure gate. Runs the pinned
# PresentMon-2.5.1-x64.exe (scripts/verify-presentmon-binary.py) against
# dwm.exe, because every windowed scene this matrix captures (classic
# Notepad, File Explorer, the packaged Notepad UWP app) composes through
# DWM's "Composed: Flip"/"Composed: Copy" path rather than presenting
# directly -- see docs/windows-arm/evidence/a2-a3-title-fps-measurement-
# 20260801.md, which measured the same thing for a different title on this
# stack. Capturing the app's own process name would see zero frames for a
# window that never owns a swapchain outside of DWM's composition.
param(
    [Parameter(Mandatory=$true)][string]$PresentMonPath,
    [Parameter(Mandatory=$true)][string]$OutputCsvPath,
    [int]$Seconds = 20
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $PresentMonPath)) { throw 'PresentMon binary not found' }
Remove-Item -LiteralPath $OutputCsvPath -Force -ErrorAction SilentlyContinue
$argumentList = @(
    '--process_name', 'dwm.exe',
    '--timed', "$Seconds",
    '--terminate_after_timed',
    '--terminate_existing_session',
    '--v2_metrics',
    '--output_file', $OutputCsvPath
)
$process = Start-Process -FilePath $PresentMonPath -ArgumentList $argumentList -PassThru -WindowStyle Hidden
$deadline = (Get-Date).AddSeconds($Seconds + 30)
while (-not $process.HasExited -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
if (-not $process.HasExited) { $process.Kill(); throw 'PresentMon capture did not terminate in time' }
if ($process.ExitCode -ne 0) { throw "PresentMon exited with code $($process.ExitCode)" }
if (-not (Test-Path -LiteralPath $OutputCsvPath)) { throw 'PresentMon produced no CSV output' }
$rowCount = (Get-Content -LiteralPath $OutputCsvPath | Measure-Object -Line).Lines
Write-Output "BVPRESENTMON path=$OutputCsvPath rows=$rowCount"
