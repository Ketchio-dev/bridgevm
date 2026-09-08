# Diagnostic-only guest asset for the B6 scale-mechanism spike
# (docs/windows-arm/evidence/b6-scale-mechanism-spike-20260908.md). Not part
# of any shipped closure gate. Sets 150% display scale; a guest restart is
# required for it to take effect (confirmed live in the linked evidence).
$ErrorActionPreference = 'Stop'
New-ItemProperty -Path 'HKCU:\Control Panel\Desktop' -Name LogPixels -Value 144 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path 'HKCU:\Control Panel\Desktop' -Name Win8DpiScaling -Value 1 -PropertyType DWord -Force | Out-Null
Write-Output 'BVSCALESET logpixels=144 win8dpiscaling=1'
