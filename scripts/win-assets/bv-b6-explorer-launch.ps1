# Diagnostic-only guest asset: launches File Explorer for the B6 tab-glyph
# scene spike (docs/windows-arm/evidence/b6-tab-scene-spike-20260908.md).
# Not part of any shipped closure gate; F1-F4 acceptance never invokes this.
$ErrorActionPreference = 'Stop'
$result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'explorer.exe' }
if ($result.ReturnValue -ne 0 -or $result.ProcessId -le 0) { throw 'Explorer CIM launch failed' }
Start-Sleep -Seconds 4
Write-Output "BVEXPLORERSTARTED pid=$($result.ProcessId)"
