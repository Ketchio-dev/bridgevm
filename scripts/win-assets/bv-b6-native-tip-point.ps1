param([long]$Hwnd, [int]$Width, [int]$Height)
$ErrorActionPreference = 'Stop'
if ($Hwnd -le 0 -or $Width -le 1 -or $Height -le 1) { throw 'Invalid tip target or display dimensions' }
Add-Type -Path (Join-Path $PSScriptRoot 'bv-b6-tip-owner.cs')
if (![BvNativeTipOwner]::Visible($Hwnd)) { throw 'Tip root is absent or hidden' }
Add-Type -Path @((Join-Path $PSScriptRoot 'bv-b6-native-uia.cs'), (Join-Path $PSScriptRoot 'bv-b6-physical-uia.cs'))
$query = [BvPhysicalUia]::Query($Hwnd)
if ($query.status -ne 'query-returned') { throw "Native UIA query failed at $($query.stage), HRESULT=$($query.hresult)" }
if ($query.truncated -or $query.match_count -ne $query.matches.Count) { throw 'Native UIA match list is incomplete' }
$visible = @($query.matches | Where-Object { !$_.is_offscreen })
if ($visible.Count -eq 0) { Write-Output "BVTIPPOINT hwnd=$Hwnd state=not-found"; return }
if ($visible.Count -ne 1) { throw 'Ambiguous visible Got it buttons' }
$button = $visible[0]; $windowProcess = [BvNativeTipOwner]::Process($Hwnd)
if (!$button.is_enabled -or $button.process_id -ne $query.root_process_id -or $query.root_process_id -ne $windowProcess) {
    throw 'Got it button is disabled or from a different process'
}
$rectangle = @($button.bounding_rectangle)
if ($rectangle.Count -ne 4) { throw 'Invalid Got it rectangle' }
foreach ($value in $rectangle) { if ([double]::IsNaN($value) -or [double]::IsInfinity($value)) { throw 'Non-finite Got it rectangle' } }
if ($rectangle[2] -le 0 -or $rectangle[3] -le 0) { throw 'Empty Got it rectangle' }
$x = [Math]::Floor($rectangle[0] + $rectangle[2] / 2)
$y = [Math]::Floor($rectangle[1] + $rectangle[3] / 2)
if ($x -lt 0 -or $y -lt 0 -or $x -ge $Width -or $y -ge $Height) { throw 'Got it point is outside the display' }
$owner = [BvNativeTipOwner]::At([int]$x, [int]$y)
if ($owner -ne $Hwnd) { throw 'Got it point is not owned by the requested window' }
Write-Output "BVTIPPOINT hwnd=$Hwnd state=present x=$x y=$y owner=$owner"
