# OPENFILE (bvagent.ps1 Open-BvDropItem): Finder's Open With and a Dock drop copy files to the
# Desktop like a drop, then open each item the drop put there. The agent takes one name that
# Test-DropName takes, directly below Get-DropRoot, that exists and is no reparse point, and hands
# its full path to explorer.exe as one quoted argument; anything else is refused before any process
# starts. READY carries `openfile`, and the resident dispatch answers on its own line. Get-DropRoot
# is a temporary folder and Start-Process is stubbed.
$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$agentPath = Join-Path $PSScriptRoot '../../scripts/win-assets/bvagent.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile($agentPath, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'resident agent parse failure' }
$names = @('Get-IoPath', 'Test-DropName', 'Open-BvDropItem', 'Get-BvReady')
$section = @($ast.EndBlock.Statements | Where-Object {
    $_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -in $names
})
if ($section.Count -ne $names.Count) { throw "expected $($names -join ', '), found $($section.Count)" }
foreach ($statement in $section) { . ([scriptblock]::Create($statement.Extent.Text)) }

$script:logged = New-Object 'System.Collections.Generic.List[string]'
function Log([string]$Message) { [void]$script:logged.Add($Message) }
$script:started = New-Object 'System.Collections.Generic.List[object]'
$script:startFails = $false
function Start-Process([string]$FilePath, [object]$ArgumentList, $ErrorAction) {
    if ($script:startFails) { throw 'explorer detail' }
    [void]$script:started.Add([pscustomobject]@{ FilePath = $FilePath; Arguments = @($ArgumentList) })
}
function B64([string]$Text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($Text)) }
$windows = $IsWindows -or $PSVersionTable.PSEdition -eq 'Desktop'

$root = Join-Path ([IO.Path]::GetTempPath()) ('bv-open-' + [Guid]::NewGuid().ToString('N'))
$outside = Join-Path ([IO.Path]::GetTempPath()) ('bv-open-outside-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
[void][IO.Directory]::CreateDirectory($outside)
function Get-DropRoot { return $root }
$cases = 0
try {
    # 1. What a drop really leaves on the Desktop opens: one explorer.exe call, the full path as its
    # one argument, in quotes with none inside.
    $hangul = [string][char]0xBCF4 + [char]0xACE0 + [char]0xC11C
    $dropped = @('notes.txt', 'notes (2).txt', "$hangul 2026.docx", 'a, b.txt', '100% done.txt', 'Photos', ' lead.txt', 'no-extension')
    foreach ($name in $dropped) {
        if ($name -ceq 'Photos') { [void][IO.Directory]::CreateDirectory((Join-Path $root $name)) }
        else { [IO.File]::WriteAllText((Join-Path $root $name), 'x') }
    }
    foreach ($name in $dropped) {
        $script:started.Clear()
        $reply = Open-BvDropItem (B64 $name)
        if ($reply -cne 'OK OPENFILE' -or $script:started.Count -ne 1) { throw "a dropped item did not open: [$name] $reply" }
        $call = $script:started[0]
        if (-not $call.FilePath.EndsWith('explorer.exe') -or $call.Arguments.Count -ne 1) { throw "not one explorer argument: [$name]" }
        $argument = [string]$call.Arguments[0]
        if ($argument -cne ('"' + [IO.Path]::Combine($root, $name) + '"') -or $argument.Split('"').Count -ne 3 -or $argument.EndsWith('\"')) { throw "argument changed: $argument" }
        $cases++
    }
    if ($windows -and $script:started[0].FilePath -cne [IO.Path]::Combine([Environment]::GetFolderPath('Windows'), 'explorer.exe')) {
        throw "not the Windows folder's explorer: $($script:started[0].FilePath)"
    }

    # 2. Nothing starts for anything that is not one dropped name: other folders, separators, quotes,
    # line breaks, control characters, reserved names, bad base64 or UTF-8.
    [IO.File]::WriteAllText((Join-Path $outside 'secret.txt'), 'x')
    $script:started.Clear()
    $invalidUtf8 = [Convert]::ToBase64String([byte[]](0x41, 0xC3, 0x28, 0x42))
    $refused = @('', 'not base64!', 'QUJD RA==', ((B64 'notes.txt') + ' extra'), $invalidUtf8, ('QUFB' * 1366),
        (B64 '.'), (B64 '..'), (B64 '..\secret.txt'), (B64 '../secret.txt'), (B64 (Join-Path $outside 'secret.txt')),
        (B64 'C:\Windows\System32\cmd.exe'), (B64 '\\server\share\x.exe'), (B64 'Photos\..\..\x'), (B64 'a/b'),
        (B64 'x" & calc & "'), (B64 '"notes.txt"'), (B64 "notes.txt`r`nRUN Y21k"), (B64 ("x" + [char]0 + 'y')),
        (B64 ("x" + [char]0x1B + 'y')), (B64 'CON'), (B64 'nul.txt'), (B64 'trailing.'), (B64 'trailing '),
        (B64 'a|b'), (B64 'a<b'), (B64 'a>b'), (B64 'a*b'), (B64 'a?b'), (B64 'C:notes.txt'))
    foreach ($arg in $refused) {
        $reply = Open-BvDropItem $arg
        if ($reply -cne 'ERR OPENFILE refused') { throw "not refused: [$arg] $reply" }
        $cases++
    }
    if ($script:started.Count -ne 0) { throw 'a refused name started a process' }

    # 3. A name that is not on the Desktop (deleted since, or never dropped) is missing, not opened.
    $reply = Open-BvDropItem (B64 'secret.txt')
    if ($reply -cne 'ERR OPENFILE missing' -or $script:started.Count -ne 0) { throw "an absent item: $reply" }
    $cases++

    # 4. A link on the Desktop that leads elsewhere is refused.
    $link = Join-Path $root 'elsewhere'
    if ($windows) { [void](New-Item -ItemType Junction -Path $link -Value $outside) }
    else { [void](New-Item -ItemType SymbolicLink -Path $link -Value $outside) }
    $reply = Open-BvDropItem (B64 'elsewhere')
    if ($reply -cne 'ERR OPENFILE refused' -or $script:started.Count -ne 0) { throw "a link was opened: $reply" }
    $cases++

    # 5. A failing launch answers failed; the detail goes to the log only.
    $script:startFails = $true
    $reply = Open-BvDropItem (B64 'notes.txt')
    if ($reply -cne 'ERR OPENFILE failed' -or -not ($script:logged -ccontains 'OPENFILE failed: explorer detail')) { throw "failed launch: $reply" }
    $script:startFails = $false
    $cases++

    # 6. READY's capability markers, before any agent= field.
    $readyTokens = (Get-BvReady $PSScriptRoot '') -split ' '
    if (-not ($readyTokens -ccontains 'openfile')) { throw 'READY does not carry openfile' }
    $cases++

    # 7. The resident dispatch answers OPENFILE on its own line.
    $switches = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.SwitchStatementAst] }, $true)
    $main = @($switches | Where-Object { @($_.Clauses | Where-Object { $_.Item1.Extent.Text -ceq "'DROPABORT'" }).Count -eq 1 })
    if ($main.Count -ne 1) { throw 'expected one resident dispatch' }
    $clause = @($main[0].Clauses | Where-Object { $_.Item1.Extent.Text -ceq "'OPENFILE'" })
    if ($clause.Count -ne 1) { throw 'expected one OPENFILE clause' }
    $dispatch = [scriptblock]::Create('switch ($tok) { ' + $clause[0].Item1.Extent.Text + ' ' + $clause[0].Item2.Extent.Text + ' }')
    function Write-Line($Handle, [string]$Line, [string]$What) {
        if ($Handle -ne 123) { throw 'resident handle changed' }
        [void]$script:wire.Add($Line)
    }
    function Invoke-ResidentToken([string]$tok, [string]$arg) { $h = 123; . $dispatch }
    $script:wire = New-Object 'System.Collections.Generic.List[string]'
    Invoke-ResidentToken 'OPENFILE' (B64 'notes.txt')
    Invoke-ResidentToken 'OPENFILE' (B64 '..\secret.txt')
    if (($script:wire.ToArray() -join '|') -cne 'OK OPENFILE|ERR OPENFILE refused') { throw ('resident wire mismatch: ' + ($script:wire.ToArray() -join '|')) }
    $cases++
} finally {
    if (Test-Path -LiteralPath (Join-Path $root 'elsewhere')) { (Get-Item -LiteralPath (Join-Path $root 'elsewhere')).Delete() }
    [IO.Directory]::Delete($root, $true)
    [IO.Directory]::Delete($outside, $true)
}
Write-Output "agent open file contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
