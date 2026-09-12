$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$path = Join-Path $PSScriptRoot 'win-assets/bvagent.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'resident agent parse failure' }
$switches = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.SwitchStatementAst] }, $true)
$clauses = @($switches | ForEach-Object { $_.Clauses } | Where-Object { $_.Item1.Extent.Text.Contains("'POINTERINPUT'") })
if ($clauses.Count -ne 1) { throw 'expected exactly one resident pointer input clause' }
$clause = $clauses[0]
$dispatch = [scriptblock]::Create('switch ($tok) { ' + $clause.Item1.Extent.Text + ' ' + $clause.Item2.Extent.Text + ' }')
function Invoke-UnicodeInput($Request, $Sender, [bool]$KeyInput, [bool]$PointerInput) {
    $script:calls++
    if ($Request -cne 'unchanged-request' -or $null -ne $Sender) { throw 'request/sender changed' }
    $script:flags = @($KeyInput, $PointerInput)
    return @{ Exit = 0; Out = 'mock-receipt' }
}
function Write-CommandResult($Handle, $Result) {
    $script:writes++
    if ($Handle -ne 123 -or $Result.Exit -ne 0 -or $Result.Out -cne 'mock-receipt') { throw 'result writer arguments changed' }
}
$h = 123; $arg = 'unchanged-request'
foreach ($tok in @('TEXTINPUT', 'KEYINPUT', 'POINTERINPUT')) {
    $script:calls = 0; $script:writes = 0; $script:flags = @()
    . $dispatch
    if ($script:calls -ne 1 -or $script:writes -ne 1 -or
        $script:flags[0] -ne ($tok -eq 'KEYINPUT') -or $script:flags[1] -ne ($tok -eq 'POINTERINPUT')) {
        throw "resident input dispatch mismatch: $tok"
    }
}
foreach ($tok in @('PING', 'POINTERINPUT extra', 'UNSUPPORTED')) {
    $script:calls = 0; $script:writes = 0
    . $dispatch
    if ($script:calls -ne 0 -or $script:writes -ne 0) { throw 'unrelated command dispatched as input' }
}
Write-Output 'PASS: actual resident input clause, three modes, request/receipt identity, unrelated-token refusal (no SendInput)'
