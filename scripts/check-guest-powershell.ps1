# Guest scripts in scripts/win-assets run under Windows PowerShell 5.1, but the Mac and CI have
# only PowerShell 7, whose parser accepts 7-only syntax, so a parse check alone passes a script
# that fails in the guest (the microphone live check's Add-Type -CompilerOptions, 2026-11-04).
# This parses each script with pwsh and flags, from the parser's tokens and ASTs, what Windows
# PowerShell 5.1 cannot run. It tests itself on two samples first.
#
#   pwsh -NoProfile -File scripts/check-guest-powershell.ps1 [path ...]
#   (default: scripts/win-assets/*.ps1)
param([Parameter(ValueFromRemainingArguments)][string[]]$Path)
$ErrorActionPreference = 'Stop'

$sevenOnlyTokens = @{
    AndAnd                 = 'the pipeline-chain operator &&'
    OrOr                   = 'the pipeline-chain operator ||'
    QuestionMark           = 'the ternary operator ? :'
    QuestionQuestion       = 'the null-coalescing operator ??'
    QuestionQuestionEquals = 'the null-coalescing assignment ??='
}
# A command's PowerShell 7-only parameter. PowerShell accepts unambiguous prefixes, so any
# prefix of two letters or more counts.
$sevenOnlyParameters = @{ 'Add-Type' = 'CompilerOptions'; 'ForEach-Object' = 'Parallel'; 'ConvertFrom-Json' = 'AsHashtable' }
$aliases = @{ 'foreach' = 'ForEach-Object'; '%' = 'ForEach-Object' }
$sevenOnlyEncodings = @('utf8NoBOM', 'utf8BOM')

function Find-SevenOnly([string]$File) {
    $tokens = $null; $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($File, [ref]$tokens, [ref]$errors)
    foreach ($e in $errors) { [pscustomobject]@{ File = $File; Line = $e.Extent.StartLineNumber; What = "parse error: $($e.Message)" } }
    foreach ($t in $tokens) {
        $what = $sevenOnlyTokens[[string]$t.Kind]
        if ($what) { [pscustomobject]@{ File = $File; Line = $t.Extent.StartLineNumber; What = "$what is PowerShell 7 only" } }
    }
    foreach ($n in $ast.FindAll({ $true }, $true)) {
        if ($n -is [System.Management.Automation.Language.ScriptBlockAst] -and $null -ne $n.CleanBlock) {
            [pscustomobject]@{ File = $File; Line = $n.CleanBlock.Extent.StartLineNumber; What = 'a clean block is PowerShell 7.3+ only' }
        } elseif ($n -is [System.Management.Automation.Language.StringConstantExpressionAst] -and $sevenOnlyEncodings -contains $n.Value) {
            [pscustomobject]@{ File = $File; Line = $n.Extent.StartLineNumber; What = "the encoding $($n.Value) is PowerShell 6+ only" }
        } elseif ($n -is [System.Management.Automation.Language.CommandAst] -and $n.GetCommandName()) {
            $name = ($n.GetCommandName() -split '\\')[-1]
            if ($aliases.ContainsKey($name)) { $name = $aliases[$name] }
            $only = $sevenOnlyParameters[$name]
            if (-not $only) { continue }
            foreach ($p in $n.CommandElements) {
                if ($p -is [System.Management.Automation.Language.CommandParameterAst] -and $p.ParameterName.Length -ge 2 -and
                    $only.StartsWith($p.ParameterName, [StringComparison]::OrdinalIgnoreCase)) {
                    [pscustomobject]@{ File = $File; Line = $p.Extent.StartLineNumber; What = "$name -$only is PowerShell 7 only" }
                }
            }
        }
    }
}

# Self-test: every construct above is flagged on its own line, and 5.1 look-alikes are not.
$bad = @'
Get-Item . && Get-Item .
Get-Item . || Get-Item .
$a = $true ? 1 : 2
$b = $null ?? 3
$c ??= 4
Add-Type -CompilerOptions '/langversion:5' -TypeDefinition 'public class A {}'
1..2 | ForEach-Object -Parallel { $_ }
1..2 | % -Par { $_ }
'{}' | ConvertFrom-Json -AsHashtable
Set-Content -Path x -Value y -Encoding utf8NoBOM
Out-File -FilePath x -Encoding:utf8BOM
function F { process { }
clean { } }
'@
$good = @'
gps | ? { $_.CPU } | % { $_ }
1..2 | ForEach-Object -Process { $_ } -End { }
1..2 | foreach -P { $_ }
Add-Type -TypeDefinition 'public class B {}' -Language CSharp
'{}' | ConvertFrom-Json
Set-Content -Path x -Value y -Encoding UTF8   # not -Encoding utf8NoBOM, && or ??
$s = 'a && b || c ?? d ? e : f'
if ($? -and $true) { }
function G { begin { } process { } end { } }
'@
$sample = New-TemporaryFile
try {
    Set-Content -LiteralPath $sample -Value $bad
    $found = (@(Find-SevenOnly $sample.FullName | Sort-Object Line | ForEach-Object Line)) -join ','
    $expected = (@(1..11) + 13) -join ','
    if ($found -ne $expected) { throw "self-test: flagged lines $found, expected $expected" }
    Set-Content -LiteralPath $sample -Value $good
    $found = @(Find-SevenOnly $sample.FullName)
    if ($found.Count) { throw "self-test: flagged Windows PowerShell 5.1 syntax: $(($found | ForEach-Object { "$($_.Line) $($_.What)" }) -join '; ')" }
} finally {
    Remove-Item -LiteralPath $sample
}

$root = Split-Path -Parent $PSScriptRoot
if (-not $Path) { $Path = @(Get-ChildItem -LiteralPath (Join-Path $root 'scripts/win-assets') -Filter *.ps1 | Sort-Object Name | ForEach-Object FullName) }
$findings = @(foreach ($file in $Path) { Find-SevenOnly (Resolve-Path -LiteralPath $file).Path })
foreach ($f in $findings) {
    $shown = [IO.Path]::GetRelativePath($root, $f.File)
    if ($shown.StartsWith('..')) { $shown = $f.File }
    '{0}:{1}: {2}' -f $shown, $f.Line, $f.What
}
if ($findings.Count) {
    "guest PowerShell: FAIL ($($findings.Count) in $($Path.Count) scripts; the guest runs Windows PowerShell 5.1)"
    exit 1
}
"guest PowerShell: PASS ($($Path.Count) scripts, Windows PowerShell 5.1 syntax)"
