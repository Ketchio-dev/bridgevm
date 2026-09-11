$ErrorActionPreference = 'Stop'
$assets = Join-Path $PSScriptRoot '../../scripts/win-assets'
Add-Type -Path @((Join-Path $assets 'bv-b6-native-uia.cs'), (Join-Path $assets 'bv-b6-physical-uia.cs'), (Join-Path $PSScriptRoot 'b6-physical-uia-contract.cs'))
$checks = [BvPhysicalUiaContract]::Verify()
if ($checks -ne 7) { throw "Expected seven DPI-scope checks, got $checks" }
Write-Output "PASS: seven native UIA DPI entry/restoration contracts; no scaled guest capture claim"
