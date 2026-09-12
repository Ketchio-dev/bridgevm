$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'win-assets'
foreach ($name in @('bv-coherence-inventory-proof.ps1', 'bv-window-inventory.cs')) {
    $text = [IO.File]::ReadAllText((Join-Path $root $name))
    if ($text -match '(?<!\r)\n') { throw 'guest asset must use CRLF' }
}
$tokens = $null; $errors = $null
$null = [Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'bv-coherence-inventory-proof.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw ($errors | Out-String) }
Add-Type -Path (Join-Path $root 'bv-window-inventory.cs') -ErrorAction Stop
if ([BridgeVM.NativeWindowInventory]::MaxRows -ne 4096 -or [BridgeVM.NativeWindowInventory]::MaxBytes -ne 4194304) { throw 'inventory protocol limits changed' }
$method = [BridgeVM.NativeWindowInventory].GetMethod('Enumerate')
if ($method.ReturnType -ne [BridgeVM.InventoryWindow[]]) { throw 'unexpected inventory API' }
Write-Output 'PASS: candidate native declarations compile; no native callback or guest window behavior proven'
