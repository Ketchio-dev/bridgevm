# Diagnostic-only guest asset for the B6 scale-mechanism spike
# (docs/windows-arm/evidence/b6-scale-mechanism-spike-20260908.md). Not part
# of any shipped closure gate.
$ErrorActionPreference = 'Stop'
$value = (Get-ItemProperty -Path 'HKCU:\Control Panel\Desktop' -Name LogPixels -ErrorAction SilentlyContinue).LogPixels
if ($null -eq $value) { $value = 'absent' }
Write-Output "BVLOGPIXELS value=$value"
