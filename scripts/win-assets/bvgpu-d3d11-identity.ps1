[CmdletBinding()]
param(
    [string]$ProcessName = "PPSSPPWindowsARM64",
    [string]$ContentPath = "C:\BridgeVMShare\cube.iso"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$process = Get-Process -Name $ProcessName -ErrorAction Stop | Select-Object -First 1
$executable = $process.Path
if (-not (Test-Path -LiteralPath $ContentPath -PathType Leaf)) { exit 2 }
Write-Output ("identity process_path={0} process_sha256={1}" -f $executable,
    (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash)
Write-Output ("identity content_path={0} content_sha256={1}" -f $ContentPath,
    (Get-FileHash -Algorithm SHA256 -LiteralPath $ContentPath).Hash)
foreach ($name in @("d3d11.dll", "dxgi.dll", "vulkan_virtio.dll")) {
    $module = @($process.Modules | Where-Object { $_.ModuleName -ieq $name } |
        Select-Object -First 1)
    if ($module.Count -ne 1) { exit 3 }
    Write-Output ("identity module={0} path={1} sha256={2}" -f $name,
        $module[0].FileName,
        (Get-FileHash -Algorithm SHA256 -LiteralPath $module[0].FileName).Hash)
}
Write-Output "identity status=PASS"
