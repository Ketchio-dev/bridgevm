# CARET (bvagent.ps1 Get-BvCaret): the caret helper compiled once in this process, only a well-formed
# BVCARET line leaves the agent, failures answer `BVCARET unavailable` with exit 1, INPUTCAPS advertises
# `BVINPUT_TEXT <id> CARET` only once the helper compiled, and the resident dispatch frames the reply as a
# RUN's. On Windows the native Find runs once for real (whatever the runner's desktop has, the line is
# well formed) and IAccessible.accLocation sits in its vtable slot.
$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$assets = Join-Path $PSScriptRoot '../../scripts/win-assets'
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $assets 'bvagent.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'resident agent parse failure' }
$section = @($ast.EndBlock.Statements | Where-Object {
    ($_ -is [Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text -like '$script:BvCaret*') -or
    ($_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -in @('Initialize-BvCaret', 'Get-BvCaret', 'Get-InputCapabilities'))
})
if ($section.Count -ne 5) { throw "expected the caret source, ready flag, Initialize-BvCaret, Get-BvCaret and Get-InputCapabilities, found $($section.Count)" }
$script:logged = New-Object 'System.Collections.Generic.List[string]'
function Log([string]$Message) { [void]$script:logged.Add($Message) }
$script:compiles = 0
function Add-Type([string]$TypeDefinition, $ErrorAction) {
    $script:compiles++
    Microsoft.PowerShell.Utility\Add-Type -TypeDefinition $TypeDefinition -ErrorAction Stop
}
foreach ($statement in $section) {
    # The dispatch below reaches the agent's function through a wrapper that stubs the native call.
    . ([scriptblock]::Create($statement.Extent.Text.Replace('function Get-BvCaret(', 'function Get-BvCaretCore(')))
}

# Only a line of four plain numbers or `none` leaves the agent; anything else is `none`.
foreach ($line in @('BVCARET none', 'BVCARET 0 0 0 1', 'BVCARET 120 340 2 18', 'BVCARET 16384 16384 16384 16384')) {
    $r = Get-BvCaretCore { $line }
    if ($r.Exit -ne 0 -or $r.Out -cne $line) { throw "well-formed caret changed: $line -> $($r.Out)" }
}
foreach ($line in @('', 'BVCARET', 'BVCARET -1 2 3 4', 'BVCARET 1 2 3', 'BVCARET 1 2 3 4 5', 'BVCARET 123456 1 1 1',
        'BVCARET 1  2 3 4', 'BVCARET 1 2 3 4 ', "BVCARET 1 2 3 4`r`n", "BVCARET 1 2 3 4`nBVAGENT END CARET", 'bvcaret none',
        'BVCARET 1.5 2 3 4', 'BVCARET 0x1 2 3 4', ([string][char]0xFF11 + ' 2 3 4'), 'BVCARET NONE')) {
    $r = Get-BvCaretCore { $line }
    if ($r.Exit -ne 0 -or $r.Out -cne 'BVCARET none') { throw "malformed caret left the agent: [$line] -> $($r.Out)" }
}
# A failing query answers unavailable; the detail goes to the log only.
$r = Get-BvCaretCore { throw 'native detail' }
if ($r.Exit -ne 1 -or $r.Out -cne 'BVCARET unavailable' -or -not ($script:logged -ccontains 'CARET failed: native detail')) { throw 'failed query not reported' }
if ($script:compiles -ne 1 -or $script:BvCaretReady -ne $true) { throw "expected one compile, saw $script:compiles" }

# The compiled helper: Find(int) -> string, and accLocation in IAccessible's slot (IUnknown 3 + IDispatch 4 + 15).
if ([BridgeVM.BvCaret].GetMethod('Find', [type[]]@([int])).ReturnType -ne [string]) { throw 'caret helper changed shape' }
$accessible = [BridgeVM.BvCaret+IAccessible]
if ($accessible.GUID -ne [guid]'618736E0-3C3D-11CF-810C-00AA00389B71' -or $accessible.GetMethods()[15].Name -cne 'accLocation') { throw 'IAccessible declaration changed' }
if ($IsWindows -or $PSVersionTable.PSEdition -eq 'Desktop') {
    $slot = [Runtime.InteropServices.Marshal]::GetComSlotForMethodInfo($accessible.GetMethod('accLocation'))
    if ($slot -ne 22) { throw "accLocation is in COM slot $slot, not 22" }
    $native = Get-BvCaretCore
    if ($native.Exit -ne 0 -or $native.Out -cnotmatch '^BVCARET (none|[0-9]+ [0-9]+ [0-9]+ [0-9]+)\z') { throw "native caret reply malformed: $($native.Out)" }
    Write-Output "native: $($native.Out)"
}

# INPUTCAPS advertises the verb only once the helper compiled.
Remove-Item Function:\Add-Type
. (Join-Path $assets 'bvagent-input.ps1')
$id = '12345678-1234-1234-1234-123456789abc'
$caps = Get-InputCapabilities $id
if ($caps.Exit -ne 0 -or -not $caps.Out.EndsWith("`nBVINPUT_TEXT $id CARET") -or -not $caps.Out.StartsWith("BVINPUT_CAPS $id 3 ")) { throw 'caret marker missing' }
$script:BvCaretReady = $false
$caps = Get-InputCapabilities $id
if ($caps.Exit -ne 0 -or $caps.Out.Contains('BVINPUT_TEXT')) { throw 'caret marker advertised without its helper' }
$r = Get-BvCaretCore { 'BVCARET 1 2 3 4' }
if ($r.Exit -ne 1 -or $r.Out -cne 'BVCARET unavailable') { throw 'uncompiled helper was called' }
$script:BvCaretReady = $true

# The resident dispatch: the CARET clause answers through Write-CommandResult, like a RUN.
$switches = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.SwitchStatementAst] }, $true)
$main = @($switches | Where-Object { @($_.Clauses | Where-Object { $_.Item1.Extent.Text -ceq "'WINCLOSE'" }).Count -eq 1 })
if ($main.Count -ne 1) { throw 'expected one resident dispatch' }
$parts = @($main[0].Clauses | Where-Object { $_.Item1.Extent.Text -ceq "'CARET'" } | ForEach-Object { $_.Item1.Extent.Text + ' ' + $_.Item2.Extent.Text })
if ($parts.Count -ne 1) { throw 'expected one CARET clause' }
$dispatch = [scriptblock]::Create('switch ($tok) { ' + $parts[0] + "`n default " + $main[0].Default.Extent.Text + ' }')
$writer = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Write-CommandResult' }, $true)
. ([scriptblock]::Create($writer.Extent.Text))
function Write-Line($Handle, [string]$Line, [string]$What) {
    if ($Handle -ne 123) { throw 'resident handle changed' }
    [void]$script:wire.Add($Line)
}
function Get-BvCaret { Get-BvCaretCore { 'BVCARET 120 340 2 18' } }
function Invoke-ResidentToken([string]$tok, [string]$arg) { $h = 123; . $dispatch }
function B64([string]$text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
$script:wire = New-Object 'System.Collections.Generic.List[string]'
Invoke-ResidentToken 'CARET' $id
Invoke-ResidentToken 'CARET' ''
$expected = @(('OUT 0 ' + (B64 'BVCARET 120 340 2 18')), ('OUT 0 ' + (B64 'BVCARET 120 340 2 18')))
if (($script:wire.ToArray() -join '|') -cne ($expected -join '|')) { throw ('resident caret wire mismatch: ' + ($script:wire.ToArray() -join '|')) }
Write-Output 'PASS: CARET reply bounds, one compile, unavailable on failure, INPUTCAPS marker only when compiled, RUN-framed dispatch'
