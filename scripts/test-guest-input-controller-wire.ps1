param([string]$Python = 'python3')
$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'win-assets'
Add-Type -Path (Join-Path $root 'bvagent-unicode-input.cs'),(Join-Path $root 'bvagent-key-input.cs'),(Join-Path $root 'bvagent-pointer-input.cs')
$json = & $Python (Join-Path $PSScriptRoot 'live-gates/guest_input_protocol.py')
if ($LASTEXITCODE -ne 0) { throw 'controller fixture generation failed' }
$rows = $json | ConvertFrom-Json
if ($rows.Count -ne 4) { throw 'incomplete diagnostic sequence' }
foreach ($row in $rows) {
    $events = switch ($row[0]) {
        'TEXTINPUT' { [BridgeVM.BvUnicodeInput]::Build([string]$row[1]) }
        'KEYINPUT' { [BridgeVM.BvUnicodeInput]::BuildKey([string]$row[1]) }
        'POINTERINPUT' { [BridgeVM.BvUnicodeInput]::BuildPointer([string]$row[1]) }
        default { throw 'unknown controller verb' }
    }
    if ($events.Count -ne [int]$row[2]) { throw 'controller count differs from guest builder' }
}
Write-Output 'PASS: exact controller sequence accepted by guest builders; no SendInput or GUI proven'
