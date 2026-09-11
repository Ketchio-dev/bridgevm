[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$OutputPath)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $OutputPath) { throw 'Inventory output already exists' }
$output = [IO.Path]::GetFullPath($OutputPath)
if (-not [IO.Directory]::Exists([IO.Path]::GetDirectoryName($output))) { throw 'Output parent must exist' }
function Get-SystemFileIdentity([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return @{ present = $false } }
    $file = Get-Item -LiteralPath $Path
    return @{ present = $true; sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant(); version = $file.VersionInfo.FileVersion }
}
$os = Get-CimInstance Win32_OperatingSystem -OperationTimeoutSec 15
$adapters = @(Get-CimInstance Win32_VideoController -OperationTimeoutSec 15 | ForEach-Object {
    @{ name = $_.Name; pnp_device_id = $_.PNPDeviceID; driver_version = $_.DriverVersion
       width = $_.CurrentHorizontalResolution; height = $_.CurrentVerticalResolution }
})
$packages = @(Get-AppxPackage -AllUsers -Name Microsoft.WindowsNotepad | ForEach-Object {
    @{ name = $_.Name; package_full_name = $_.PackageFullName; version = [string]$_.Version; architecture = [string]$_.Architecture }
})
$report = [ordered]@{
    schema_version = 1; observation_only = $true; reference_accepted = $false; criterion_pass = $false
    captured_utc = [DateTime]::UtcNow.ToString('o')
    os = @{ version = $os.Version; build = $os.BuildNumber; os_architecture = $os.OSArchitecture }
    collector_process_architecture = $env:PROCESSOR_ARCHITECTURE
    display_adapters = $adapters; installed_notepad_packages = $packages
    d3dconfig = Get-SystemFileIdentity (Join-Path $env:SystemRoot 'System32\d3dconfig.exe')
    consolas_regular = Get-SystemFileIdentity (Join-Path $env:SystemRoot 'Fonts\consola.ttf')
    limitations = @('Inventory does not identify the active application renderer', 'Installed package versions do not identify the running package', 'No independent glyph comparison or frame-time baseline', 'D3DConfig was not executed; no graphics settings changed')
}
$bytes = (New-Object Text.UTF8Encoding($false)).GetBytes(($report | ConvertTo-Json -Depth 6) + "`n")
if ($bytes.Length -gt 65536) { throw 'Inventory exceeds 64 KiB' }
$stream = [IO.File]::Open($output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
