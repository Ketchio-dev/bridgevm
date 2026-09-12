$ErrorActionPreference = 'Stop'
$assets = Join-Path $PSScriptRoot 'win-assets'
Add-Type -Path (Join-Path $assets 'bv-window-inventory.cs') -ErrorAction Stop
. (Join-Path $assets 'bvagent-window-inventory.ps1')
function New-NativeRow([string]$handle) {
    $row = [BridgeVM.InventoryWindow]::new()
    $row.Handle = $handle; $row.ProcessId = 7
    $row.X = -10; $row.Y = 20; $row.Width = 300; $row.Height = 180
    $row.Title = 'one'
    return $row
}
$first = New-NativeRow '42'; $second = New-NativeRow '43'
$second.X = 310; $second.Title = 'two'
$lines = @(ConvertTo-BvWindowInventoryLines -Rows @($first, $second))
if ($lines.Count -ne 2 -or $lines[0] -cne 'WIN 42 7 -10 20 300 180 b25l' -or
        $lines[1] -cne 'WIN 43 7 310 20 300 180 dHdv') {
    throw 'Shipped native row declarations do not match exact inventory wire'
}
foreach ($field in @('Width', 'Height')) {
    $bad = New-NativeRow '44'; $bad.$field = 0
    $seen = [Collections.Generic.List[string]]::new()
    $failed = $false
    try { ConvertTo-BvWindowInventoryLines -Rows @($first, $bad) | ForEach-Object { $seen.Add($_) } }
    catch { $failed = $true }
    if (-not $failed -or $seen.Count -ne 0) { throw 'Invalid native bounds leaked inventory rows' }
}
Write-Output 'PASS: shipped C# row declarations match wire; no native enumeration or resident-agent proof'
