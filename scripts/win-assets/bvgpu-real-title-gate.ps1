[CmdletBinding()]
param(
    [string]$Executable = "C:\BridgeVM\apps\ppsspp\PPSSPPWindowsARM64.exe",
    [ValidateRange(10, 600)]
    [int]$MinimumSeconds = 30,
    [string]$LogPath = "C:\BridgeVM\bvgpu-real-title-gate.log"
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
$process = Start-Process -FilePath $Executable -WorkingDirectory $appDirectory -PassThru
Write-GateLog "process_started pid=$($process.Id) executable_sha256=$executableHash"

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

$elapsedMs = [int]([DateTime]::UtcNow - $startedAt).TotalMilliseconds
Write-GateLog "status=PASS pid=$($process.Id) elapsed_ms=$elapsedMs venus_icd=$venusModulePath main_window_observed=true"
Write-GateLog "BVGPU-REAL-TITLE-PASS"
Exit-Gate 0
