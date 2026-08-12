[CmdletBinding()]
param(
    [string]$ProcessName = "PPSSPPWindowsARM64",
    [string]$ContentPath = "C:\BridgeVMShare\cube.iso"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
function Get-Sha { param([string]$Path) (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash }
# An exiting match reports a null Path; hidden module props trip StrictMode.
$live = @(Get-Process -Name $ProcessName -ErrorAction Stop |
    Where-Object { -not $_.HasExited -and $_.Path } | Select-Object -First 1)
if ($live.Count -ne 1 -or -not (Test-Path -LiteralPath $ContentPath -PathType Leaf)) { exit 2 }
$executable = $live[0].Path
Write-Output ("identity process_path={0} process_sha256={1}" -f $executable, (Get-Sha $executable))
Write-Output ("identity content_path={0} content_sha256={1}" -f $ContentPath, (Get-Sha $ContentPath))
$live[0].Refresh()
$named = @($live[0].Modules | Where-Object { $_ -and $_.PSObject.Properties.Match('ModuleName').Count })
foreach ($name in @("d3d11.dll", "dxgi.dll", "vulkan_virtio.dll")) {
    # Report every module with this base name; a process can map two.
    $all = @($named | Where-Object { $_.ModuleName -ieq $name }); if (!$all.Count) { exit 3 }
    $all | ForEach-Object { Write-Output ("identity module={0} path={1} sha256={2}" -f $name, $_.FileName, (Get-Sha $_.FileName)) }
}
Write-Output "identity status=PASS"
