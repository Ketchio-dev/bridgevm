$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$path = Join-Path $PSScriptRoot 'win-assets/bvagent.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'resident agent parse failure' }
$switches = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.SwitchStatementAst] }, $true)
$parts = @()
foreach ($verb in @('WINLIST', 'PING')) {
    $clauses = @($switches | ForEach-Object { $_.Clauses } | Where-Object { $_.Item1.Extent.Text -ceq "'$verb'" })
    if ($clauses.Count -ne 1) { throw "expected exactly one resident $verb clause" }
    $parts += $clauses[0].Item1.Extent.Text + ' ' + $clauses[0].Item2.Extent.Text
}
$dispatch = [scriptblock]::Create('switch ($tok) { ' + ($parts -join "`n") + ' }')
$script:rows = @('WIN 42 7 -10 20 300 180 b25l', 'WIN 43 7 40 60 200 100 dHdv')
function Log([string]$Message) { }
function Get-BvWindowInventoryLines {
    $script:providerCalls++
    switch ($script:mode) {
        'empty' { return }
        'throw' { throw 'private provider detail C:\private\guest' }
        'partial-throw' { Write-Output $script:rows[0]; throw 'private partial provider failure' }
        default { return $script:rows }
    }
}
function Write-Line($Handle, [string]$Line, [string]$What) {
    if ($Handle -ne 123) { throw 'resident handle changed' }
    [void]$script:wire.Add($Line)
}
function Invoke-ResidentToken([string]$tok) {
    $ErrorActionPreference = 'Continue'
    $h = 123
    . $dispatch
}
foreach ($script:mode in @('valid', 'empty', 'throw', 'partial-throw')) {
    $script:providerCalls = 0
    $script:wire = New-Object 'System.Collections.Generic.List[string]'
    Invoke-ResidentToken 'WINLIST'
    $expected = if ($script:mode -eq 'valid') { @($script:rows) + @('WINEND') }
        elseif ($script:mode -eq 'empty') { @('WINEND') } else { @('ERR WINLIST unavailable') }
    if (($script:wire.ToArray() -join '|') -cne ($expected -join '|')) { throw "resident wire mismatch: $script:mode" }
    Invoke-ResidentToken 'PING'
    $expected = @($expected) + @('PONG')
    if ($script:providerCalls -ne 1 -or ($script:wire.ToArray() -join '|') -cne ($expected -join '|')) {
        throw "subsequent PING or provider call count changed: $script:mode"
    }
}
Remove-Item Function:\Get-BvWindowInventoryLines -ErrorAction Stop
$statements = $ast.EndBlock.Statements
$indices = @(for ($i = 0; $i -lt $statements.Count; $i++) {
    if ($statements[$i] -is [Management.Automation.Language.TryStatementAst] -and
        $statements[$i].Extent.Text.Contains('bvagent-window-inventory.ps1')) { $i }
})
if ($indices.Count -ne 1 -or $indices[0] -eq 0) { throw 'expected one optional inventory startup guard' }
$index = $indices[0]
$initialize = [scriptblock]::Create($statements[$index - 1].Extent.Text + "`n" + $statements[$index].Extent.Text)
function Check-Startup([string]$Root, [bool]$Unavailable) {
    $startup = Join-Path $Root 'resident-startup.ps1'; [IO.File]::WriteAllText($startup, $initialize.ToString(), [Text.Encoding]::ASCII)
    $ErrorActionPreference = 'Continue'
    . $startup
    if ($ErrorActionPreference -cne 'Continue') { throw 'startup changed caller error policy' }
    $failed = $false; $result = @()
    try { $result = @(Get-BvWindowInventoryLines) }
    catch { if ($_.Exception.Message -cne 'window-inventory-unavailable') { throw }; $failed = $true }
    if ($failed -ne $Unavailable -or $result.Count -ne 0) { throw 'optional inventory startup did not fail closed' }
}
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('bv-inventory-startup-' + [guid]::NewGuid().ToString('N'))
try {
    foreach ($kind in @('missing', 'invalid', 'empty', 'valid')) {
        $directory = Join-Path $temporary $kind
        [void](New-Item -ItemType Directory -Path $directory -Force -ErrorAction Stop)
        $module = Join-Path $directory 'bvagent-window-inventory.ps1'
        if ($kind -eq 'invalid') { [IO.File]::WriteAllText($module, 'function Broken {', [Text.Encoding]::ASCII) }
        if ($kind -eq 'empty') { [IO.File]::WriteAllText($module, '# no inventory entrypoint', [Text.Encoding]::ASCII) }
        if ($kind -eq 'valid') { [IO.File]::WriteAllText($module, 'function Get-BvWindowInventoryLines { return @() }', [Text.Encoding]::ASCII) }
        Check-Startup $directory ($kind -ne 'valid')
    }
} finally {
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction Stop }
}
Write-Output 'PASS: actual resident inventory dispatch, whole-list buffering, ERR-only failure, PING continuity and optional startup guards (no native calls)'
