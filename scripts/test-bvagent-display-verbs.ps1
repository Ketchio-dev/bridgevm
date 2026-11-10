# DISPLAYMODE <w> <h> and DISPLAYSCALE <p> (bvagent.ps1 Invoke-BvDisplay): strict
# arguments, the probe's C# compiled once in this process, a reply framed like a
# RUN's, the ERR lines the host falls back on, and the resident dispatch. The
# native Run is stubbed: no display setting is read or changed.
$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$path = Join-Path $PSScriptRoot 'win-assets/bvagent.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'resident agent parse failure' }
$section = @($ast.EndBlock.Statements | Where-Object {
    ($_ -is [Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text -like '$script:BvDisplay*') -or
    ($_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -eq 'Invoke-BvDisplay')
})
if ($section.Count -ne 4) { throw "expected the display sources, ready table, scales and Invoke-BvDisplay, found $($section.Count)" }
$script:logged = New-Object 'System.Collections.Generic.List[string]'
function Log([string]$Message) { [void]$script:logged.Add($Message) }
$script:compiles = 0
function Add-Type([string]$TypeDefinition, $ErrorAction) {
    $script:compiles++
    Microsoft.PowerShell.Utility\Add-Type -TypeDefinition $TypeDefinition -ErrorAction Stop
}
foreach ($statement in $section) {
    # The dispatch below reaches the agent's function through a wrapper that stubs the native call.
    . ([scriptblock]::Create($statement.Extent.Text.Replace('function Invoke-BvDisplay(', 'function Invoke-BvDisplayCore(')))
}
$script:calls = New-Object 'System.Collections.Generic.List[string]'
$script:stub = {
    param($v)
    [void]$script:calls.Add((@($v | ForEach-Object { $_.GetType().Name + ':' + $_ }) -join ','))
    if ($v.Count -eq 2) { 'BVDISPLAY target=' + $v[0] + 'x' + $v[1] + ' result=0' } else { 'BVDPI target=' + $v[0] + ' result=0' }
}

# Valid arguments reach Run as integers; the first use of each verb compiles its class, once.
$valid = @(@('DISPLAYMODE', '1920 1080', 'Int32:1920,Int32:1080', 'BVDISPLAY target=1920x1080 result=0'),
    @('DISPLAYMODE', '320 320', 'Int32:320,Int32:320', 'BVDISPLAY target=320x320 result=0'),
    @('DISPLAYMODE', '16384 16384', 'Int32:16384,Int32:16384', 'BVDISPLAY target=16384x16384 result=0'))
foreach ($percent in @(100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500)) {
    $valid += , @('DISPLAYSCALE', [string]$percent, ('Int32:' + $percent), ('BVDPI target=' + $percent + ' result=0'))
}
foreach ($case in $valid) {
    $script:calls.Clear()
    $r = Invoke-BvDisplayCore $case[0] $case[1] $script:stub
    if ($r -isnot [hashtable] -or $r.Exit -ne 0 -or $r.Out -cne ($case[3] + "`r`n") -or ($script:calls -join '|') -cne $case[2]) {
        throw ('valid display request mismatch: ' + $case[0] + ' ' + $case[1])
    }
}
if ($script:compiles -ne 2 -or $script:BvDisplayReady['DISPLAYMODE'] -ne $true -or $script:BvDisplayReady['DISPLAYSCALE'] -ne $true) {
    throw "expected one compile per verb, saw $script:compiles"
}
# The compiled classes are the probe's: public static string Run(int, int) and Run(int).
if ([BvD].GetMethod('Run', [type[]]@([int], [int])).ReturnType -ne [string] -or
    [BvS].GetMethod('Run', [type[]]@([int])).ReturnType -ne [string]) { throw 'compiled display classes changed shape' }

# Anything else is refused before any compile or call.
$refused = @('319 1080', '1920 16385', '99999 1080', '01920 1080', '+1920 1080', '-1920 1080', '1920  1080', ' 1920 1080',
    '1920 1080 ', '1920x1080', '1920', '', '1920 1080 1', "1920 1080`n", "1920`t1080", '1920.0 1080',
    ([string][char]0xFF11 + '920 1080')) | ForEach-Object { , @('DISPLAYMODE', $_) }
$refused += @('0', '99', '101', '120', '550', '0100', '+100', '100 ', ' 100', '', '100%', "100`n", '1e2', '1920 1080') |
    ForEach-Object { , @('DISPLAYSCALE', $_) }
foreach ($case in $refused) {
    $script:calls.Clear()
    $r = Invoke-BvDisplayCore $case[0] $case[1] $script:stub
    if ($r -cne ('ERR ' + $case[0] + ' args') -or $script:calls.Count -ne 0) { throw ('display request accepted: ' + $case[0] + ' [' + $case[1] + ']') }
}

# A failing Run answers ERR <verb> failed, the detail goes to the log only.
$r = Invoke-BvDisplayCore 'DISPLAYMODE' '1920 1080' { param($v) throw 'native detail' }
if ($r -cne 'ERR DISPLAYMODE failed' -or -not ($script:logged -ccontains 'DISPLAYMODE failed: native detail')) { throw 'failed Run not reported' }
# A class that does not compile answers ERR <verb> unavailable, and is not compiled again.
$script:BvDisplayReady.Remove('DISPLAYSCALE')
$script:BvDisplaySource['DISPLAYSCALE'] = 'public static class {'
$before = $script:compiles
foreach ($attempt in 1..2) {
    $script:calls.Clear()
    $r = Invoke-BvDisplayCore 'DISPLAYSCALE' '150' $script:stub
    if ($r -cne 'ERR DISPLAYSCALE unavailable' -or $script:calls.Count -ne 0) { throw 'uncompiled class was called' }
}
if ($script:compiles -ne $before + 1) { throw 'a failed compile was retried' }

# The resident dispatch: the display clause, PING and the default branch an older agent answers with.
$switches = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.SwitchStatementAst] }, $true)
$main = @($switches | Where-Object { @($_.Clauses | Where-Object { $_.Item1.Extent.Text -ceq "'WINCLOSE'" }).Count -eq 1 })
if ($main.Count -ne 1) { throw 'expected one resident dispatch' }
$parts = @($main[0].Clauses | Where-Object { $_.Item1.Extent.Text -ceq "'PING'" -or $_.Item1.Extent.Text.Contains("'DISPLAYMODE'") } |
    ForEach-Object { $_.Item1.Extent.Text + ' ' + $_.Item2.Extent.Text })
if ($parts.Count -ne 2) { throw 'expected one display clause and one PING clause' }
$dispatch = [scriptblock]::Create('switch ($tok) { ' + ($parts -join "`n") + "`n default " + $main[0].Default.Extent.Text + ' }')
$writer = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Write-CommandResult' }, $true)
. ([scriptblock]::Create($writer.Extent.Text))
function Write-Line($Handle, [string]$Line, [string]$What) {
    if ($Handle -ne 123) { throw 'resident handle changed' }
    [void]$script:wire.Add($Line)
}
$script:BvDisplayReady['DISPLAYSCALE'] = $true  # compiled above, before the failed-compile case
function Invoke-BvDisplay([string]$Verb, [string]$Arg) { Invoke-BvDisplayCore $Verb $Arg $script:stub }
function Invoke-ResidentToken([string]$tok, [string]$arg) {
    $h = 123
    . $dispatch
}
function B64([string]$text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
$script:wire = New-Object 'System.Collections.Generic.List[string]'
Invoke-ResidentToken 'DISPLAYMODE' '2560 1600'
Invoke-ResidentToken 'DISPLAYSCALE' '150'
Invoke-ResidentToken 'DISPLAYMODE' '2560'
Invoke-ResidentToken 'displaymode' '2560 1600'
Invoke-ResidentToken 'PING' ''
$expected = @(('OUT 0 ' + (B64 "BVDISPLAY target=2560x1600 result=0`r`n")), ('OUT 0 ' + (B64 "BVDPI target=150 result=0`r`n")),
    'ERR DISPLAYMODE args', ('OUT 255 ' + (B64 'unknown token: displaymode')), 'PONG')
if (($script:wire.ToArray() -join '|') -cne ($expected -join '|')) { throw ('resident display wire mismatch: ' + ($script:wire.ToArray() -join '|')) }
Write-Output 'PASS: DISPLAYMODE/DISPLAYSCALE argument refusal, one compile per class, RUN-framed replies, ERR fallback lines and resident dispatch (no display change)'
