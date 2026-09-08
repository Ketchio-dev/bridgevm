# Guest asset for the B6 glyph matrix: sets a declared per-monitor display
# scale via the registry-plus-restart mechanism proven live in
# docs/windows-arm/evidence/b6-scale-mechanism-spike-20260908.md. A guest
# restart is required for LogPixels to take effect; the matrix harness issues
# it separately. -LogPixels must be one of the three declared matrix values:
# 96 (100%), 120 (125%), 144 (150%).
param([Parameter(Mandatory=$true)][ValidateSet(96,120,144)][int]$LogPixels)
$ErrorActionPreference = 'Stop'
New-ItemProperty -Path 'HKCU:\Control Panel\Desktop' -Name LogPixels -Value $LogPixels -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path 'HKCU:\Control Panel\Desktop' -Name Win8DpiScaling -Value 1 -PropertyType DWord -Force | Out-Null
Write-Output "BVSCALESET logpixels=$LogPixels win8dpiscaling=1"
