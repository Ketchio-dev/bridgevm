[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ManifestPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class BridgeVMTitleWindow {
    [DllImport("user32.dll")]
    public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
}
"@

function Fail-Manifest {
    param([Parameter(Mandatory = $true)][string]$Reason)
    throw "invalid title manifest: $Reason"
}

function Require-String {
    param([Parameter(Mandatory = $true)]$Object, [Parameter(Mandatory = $true)][string]$Name)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property -or $property.Value -isnot [string] -or [string]::IsNullOrWhiteSpace($property.Value)) {
        Fail-Manifest "$Name must be a non-empty string"
    }
    return [string]$property.Value
}

function Get-PeArchitecture {
    param([Parameter(Mandatory = $true)][string]$Path)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5A4D) { return "unknown" }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        if ($peOffset -gt ($stream.Length - 6)) { return "unknown" }
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) { return "unknown" }
        switch ($reader.ReadUInt16()) {
            0xAA64 { return "arm64" }
            0x8664 { return "x64" }
            default { return "unknown" }
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Get-LoadedModuleNames {
    param([Parameter(Mandatory = $true)][Diagnostics.Process]$Process)
    try {
        $Process.Refresh()
        return @($Process.Modules | ForEach-Object { $_.ModuleName.ToLowerInvariant() })
    }
    catch {
        return @()
    }
}

if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    Fail-Manifest "file not found: $ManifestPath"
}
$manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
if ($manifest.version -ne 1) { Fail-Manifest "version must be 1" }
$id = Require-String $manifest "id"
if ($id -notmatch '^[A-Za-z0-9._-]+$') { Fail-Manifest "id contains unsupported characters" }
$api = (Require-String $manifest "api").ToLowerInvariant()
if ($api -notin @("vulkan", "d3d11", "d3d12")) { Fail-Manifest "api must be vulkan, d3d11, or d3d12" }
$architecture = (Require-String $manifest "architecture").ToLowerInvariant()
if ($architecture -notin @("arm64", "x64")) { Fail-Manifest "architecture must be arm64 or x64" }
$executable = Require-String $manifest "executable"
$logName = Require-String $manifest "log"
if ([IO.Path]::GetFileName($logName) -ne $logName -or $logName -in @(".", "..")) {
    Fail-Manifest "log must be a file name without directories"
}
$passMarker = Require-String $manifest "pass_marker"
if ($passMarker.Contains("`r") -or $passMarker.Contains("`n")) { Fail-Manifest "pass_marker must be one line" }
$minimumSeconds = [uint64]$manifest.minimum_runtime_seconds
if ($minimumSeconds -lt 1 -or $minimumSeconds -gt 86400) { Fail-Manifest "minimum_runtime_seconds must be 1..86400" }
$requireMainWindow = if ($null -eq $manifest.PSObject.Properties["require_main_window"]) { $true } else { [bool]$manifest.require_main_window }
$requiredModulesValue = if ($null -eq $manifest.PSObject.Properties["required_modules"]) { @() } else { @($manifest.required_modules) }
$requiredModules = @($requiredModulesValue | ForEach-Object {
    if ($_ -isnot [string] -or [string]::IsNullOrWhiteSpace($_)) { Fail-Manifest "required_modules entries must be non-empty strings" }
    ([IO.Path]::GetFileName([string]$_)).ToLowerInvariant()
})
$arguments = if ($null -eq $manifest.PSObject.Properties["arguments"]) { @() } else { @($manifest.arguments | ForEach-Object { [string]$_ }) }
$workingDirectory = if ($null -eq $manifest.PSObject.Properties["working_directory"]) {
    Split-Path -Parent $executable
} else {
    Require-String $manifest "working_directory"
}

$bridgeVMDirectory = "C:\BridgeVM"
$logPath = Join-Path $bridgeVMDirectory $logName
New-Item -ItemType Directory -Force -Path $bridgeVMDirectory | Out-Null
[IO.File]::WriteAllText($logPath, "")
function Write-GateLog {
    param([Parameter(Mandatory = $true)][string]$Message)
    $line = "[bvgpu-title-gate] $Message"
    Write-Output $line
    [IO.File]::AppendAllText($logPath, $line + [Environment]::NewLine)
}

$mutex = [Threading.Mutex]::new($false, "Global\BridgeVMTitleGate-$id")
$acquired = $false
try {
    try { $acquired = $mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $acquired = $true }
    if (-not $acquired) { Write-GateLog "status=FAIL reason=already-running"; exit 7 }
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        Write-GateLog "status=FAIL reason=executable-missing executable=$executable"
        exit 2
    }
    $observedArchitecture = Get-PeArchitecture $executable
    if ($observedArchitecture -ne $architecture) {
        Write-GateLog "status=FAIL reason=architecture-mismatch expected=$architecture observed=$observedArchitecture"
        exit 3
    }
    $executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
    if ($null -ne $manifest.PSObject.Properties["executable_sha256"]) {
        $expectedHash = ([string]$manifest.executable_sha256).ToLowerInvariant()
        if ($expectedHash -notmatch '^[0-9a-f]{64}$' -or $expectedHash -ne $executableHash) {
            Write-GateLog "status=FAIL reason=executable-sha256-mismatch executable_sha256=$executableHash"
            exit 4
        }
    }

    $startedAt = [DateTime]::UtcNow
    Write-GateLog "status=START id=$id api=$api architecture=$architecture executable_sha256=$executableHash"
    $process = Start-Process -FilePath $executable -ArgumentList $arguments -WorkingDirectory $workingDirectory -PassThru
    Write-GateLog "process_started pid=$($process.Id)"
    $deadline = $startedAt.AddSeconds([double]$minimumSeconds)
    $observedModules = @{}
    $mainWindowObserved = $false
    while ([DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
        if ($process.HasExited) {
            $elapsedMs = [uint64]([DateTime]::UtcNow - $startedAt).TotalMilliseconds
            Write-GateLog "status=FAIL reason=process-exited exit_code=$($process.ExitCode) elapsed_ms=$elapsedMs"
            exit 5
        }
        foreach ($module in (Get-LoadedModuleNames $process)) { $observedModules[$module] = $true }
        if (-not $mainWindowObserved -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
            [void][BridgeVMTitleWindow]::ShowWindowAsync($process.MainWindowHandle, 9)
            $mainWindowObserved = $true
            Write-GateLog "main_window_observed=true handle=$($process.MainWindowHandle)"
        }
    }

    $missingModules = @($requiredModules | Where-Object { -not $observedModules.ContainsKey($_) })
    if ($missingModules.Count -gt 0) {
        Write-GateLog "status=FAIL reason=required-modules-missing modules=$($missingModules -join ',')"
        exit 6
    }
    if ($requireMainWindow -and -not $mainWindowObserved) {
        Write-GateLog "status=FAIL reason=main-window-not-observed"
        exit 8
    }
    $elapsedMs = [uint64]([DateTime]::UtcNow - $startedAt).TotalMilliseconds
    foreach ($module in $requiredModules) { Write-GateLog "module=$module" }
    Write-GateLog "status=PASS pid=$($process.Id) elapsed_ms=$elapsedMs main_window_observed=$($mainWindowObserved.ToString().ToLowerInvariant()) executable_sha256=$executableHash"
    Write-GateLog $passMarker
    exit 0
}
finally {
    if ($acquired) { try { $mutex.ReleaseMutex() } catch { } }
    $mutex.Dispose()
}
