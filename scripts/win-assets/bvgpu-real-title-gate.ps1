[CmdletBinding()]
param(
    [string]$Executable = "C:\BridgeVM\apps\ppsspp\PPSSPPWindowsARM64.exe",
    [ValidateRange(10, 600)]
    [int]$MinimumSeconds = 30,
    [string]$LogPath = "C:\BridgeVM\bvgpu-real-title-gate.log",
    # Game content to boot. Without content PPSSPP sits at its menu and
    # emits no "fps:" lines, so guest_fps reads samples=0 (measured
    # 2026-08-06); the frame counter runs only while a title renders.
    [string]$ContentPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class BridgeVMWindowActivation {
    [DllImport("user32.dll")]
    public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@

function Write-GateLog {
    param([Parameter(Mandatory = $true)][string]$Message)

    $line = "[bvgpu-real-title] $Message"
    Write-Output $line
    [IO.File]::AppendAllText($LogPath, $line + [Environment]::NewLine)
}

function Get-LoadedModulePath {
    param(
        [Parameter(Mandatory = $true)][Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$ModuleName
    )

    try {
        $Process.Refresh()
        $module = @($Process.Modules | Where-Object { $_.ModuleName -ieq $ModuleName } | Select-Object -First 1)
        if ($module.Count -eq 1) {
            return $module[0].FileName
        }
    }
    catch {
        Write-GateLog "module_probe_retry error=$($_.Exception.Message)"
    }
    return $null
}

$gateMutex = [Threading.Mutex]::new($false, "Global\BridgeVMGpuRealTitleGate")
$gateMutexAcquired = $false
try {
    $gateMutexAcquired = $gateMutex.WaitOne(0)
}
catch [Threading.AbandonedMutexException] {
    $gateMutexAcquired = $true
}
if (-not $gateMutexAcquired) {
    $gateMutex.Dispose()
    exit 7
}

function Exit-Gate {
    param([Parameter(Mandatory = $true)][int]$Code)

    if ($script:gateMutexAcquired) {
        try { $script:gateMutex.ReleaseMutex() } catch { }
        $script:gateMutexAcquired = $false
    }
    $script:gateMutex.Dispose()
    exit $Code
}

[IO.File]::WriteAllText($LogPath, "")
$startedAt = [DateTime]::UtcNow
Write-GateLog "status=START utc=$($startedAt.ToString('o')) executable=$Executable minimum_seconds=$MinimumSeconds"

if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    Write-GateLog "status=FAIL reason=executable-missing"
    Exit-Gate 2
}

$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Executable).Hash
Get-Process -Name "PPSSPPWindowsARM64" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 1000
$appDirectory = Split-Path -Parent $Executable
$portableSystem = Join-Path $appDirectory "memstick\PSP\SYSTEM"
$portablePoison = Join-Path $portableSystem "FailedGraphicsBackends.txt"
$canonicalConfig = Join-Path $appDirectory "bv-ppsspp.ini"
New-Item -ItemType Directory -Force -Path $portableSystem | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $portablePoison
$userSystem = "C:\Users\bridge\Documents\PPSSPP\PSP\SYSTEM"
Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath (Join-Path $userSystem "FailedGraphicsBackends.txt")
if (Test-Path -LiteralPath $canonicalConfig -PathType Leaf) {
    Copy-Item -Force -LiteralPath $canonicalConfig -Destination (Join-Path $portableSystem "ppsspp.ini")
}
Write-GateLog "launch_state_clean portable_poison_removed=$(-not (Test-Path -LiteralPath $portablePoison))"
# Frame rate has to be measured here, in the guest. The host's RESOURCE_FLUSH
# rate is not this program's frame rate: every flush in a reference run was
# issued on ctx 0 by the desktop compositor while the title rendered on its own
# context, so the host counts DWM's scanout and cannot see the app's frames.
# PPSSPP emits "delta: <ms> fps: <n>" into its own log, so let it self-report.
$frameLog = Join-Path $appDirectory "bv-frame.log"
Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $frameLog
$launchArgs = @("--log=$frameLog", "--loglevel=info")
if ($ContentPath -ne "") {
    if (-not (Test-Path -LiteralPath $ContentPath -PathType Leaf)) {
        Write-GateLog "status=FAIL reason=content-missing path=$ContentPath"
        Exit-Gate 7
    }
    $launchArgs += $ContentPath
}
$process = Start-Process -FilePath $Executable -WorkingDirectory $appDirectory -PassThru -ArgumentList $launchArgs
Write-GateLog "process_started pid=$($process.Id) executable_sha256=$executableHash frame_log=$frameLog"

$deadline = $startedAt.AddSeconds($MinimumSeconds)
$venusModulePath = $null
$mainWindowObserved = $false
while ([DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 250
    $process.Refresh()
    if ($process.HasExited) {
        $elapsedMs = [int]([DateTime]::UtcNow - $startedAt).TotalMilliseconds
        Write-GateLog "status=FAIL reason=process-exited pid=$($process.Id) exit_code=$($process.ExitCode) elapsed_ms=$elapsedMs"
        Exit-Gate 3
    }
    if ($null -eq $venusModulePath) {
        $venusModulePath = Get-LoadedModulePath -Process $process -ModuleName "vulkan_virtio.dll"
        if ($null -ne $venusModulePath) {
            Write-GateLog "venus_icd_loaded path=$venusModulePath"
        }
    }
    if (-not $mainWindowObserved -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
        $window = $process.MainWindowHandle
        $restored = [BridgeVMWindowActivation]::ShowWindowAsync($window, 9)
        $foreground = [BridgeVMWindowActivation]::SetForegroundWindow($window)
        $mainWindowObserved = $true
        Write-GateLog "main_window_observed handle=$window restored=$restored foreground=$foreground"
    }
}

$process.Refresh()
if ($process.HasExited) {
    Write-GateLog "status=FAIL reason=process-exited-at-deadline pid=$($process.Id) exit_code=$($process.ExitCode)"
    Exit-Gate 4
}
if ($null -eq $venusModulePath) {
    Write-GateLog "status=FAIL reason=venus-icd-not-loaded pid=$($process.Id)"
    Exit-Gate 5
}
if (-not $mainWindowObserved) {
    Write-GateLog "status=FAIL reason=main-window-not-observed pid=$($process.Id)"
    Exit-Gate 6
}

# Report what the title itself measured. This is observation only: no FPS
# threshold gates the result yet, because no baseline has been established on
# this stack. Absence of samples is reported as samples=0 rather than as a
# failure, so an unrelated logging change cannot silently fail the title gate.
$fpsSamples = @()
if (Test-Path -LiteralPath $frameLog -PathType Leaf) {
    # The title still holds its log open for writing; ReadAllLines opens with
    # no write sharing and dies with a sharing violation (seen live
    # 2026-08-06). Open ReadWrite-shared instead.
    $stream = [IO.File]::Open($frameLog, [IO.FileMode]::Open,
        [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        $reader = New-Object IO.StreamReader($stream)
        while ($null -ne ($line = $reader.ReadLine())) {
            $m = [regex]::Match($line, 'fps:\s*([0-9]+(?:\.[0-9]+)?)')
            if ($m.Success) { $fpsSamples += [double]$m.Groups[1].Value }
        }
    } finally {
        $stream.Dispose()
    }
}
if ($fpsSamples.Count -gt 0) {
    $sorted = $fpsSamples | Sort-Object
    $p50 = $sorted[[int][Math]::Floor(($sorted.Count - 1) * 0.5)]
    $p05 = $sorted[[int][Math]::Floor(($sorted.Count - 1) * 0.05)]
    $mean = ($fpsSamples | Measure-Object -Average).Average
    Write-GateLog ("guest_fps samples={0} p50={1:F2} p05={2:F2} mean={3:F2} min={4:F2} max={5:F2}" -f `
        $fpsSamples.Count, $p50, $p05, $mean, $sorted[0], $sorted[$sorted.Count - 1])
} else {
    Write-GateLog "guest_fps samples=0 reason=no-fps-lines-in-frame-log"
}

$elapsedMs = [int]([DateTime]::UtcNow - $startedAt).TotalMilliseconds
Write-GateLog "status=PASS pid=$($process.Id) elapsed_ms=$elapsedMs venus_icd=$venusModulePath main_window_observed=true"
Write-GateLog "BVGPU-REAL-TITLE-PASS"
Exit-Gate 0
