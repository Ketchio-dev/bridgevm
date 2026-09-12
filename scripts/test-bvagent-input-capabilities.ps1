$ErrorActionPreference = 'Stop'
$agent = Join-Path $PSScriptRoot 'win-assets/bvagent.ps1'
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($agent, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'capability agent parse failure' }
$definition = $ast.Find({ param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -eq 'Get-InputCapabilities'
}, $true)
if ($null -eq $definition) { throw 'missing resident capability function' }
. ([scriptblock]::Create($definition.Extent.Text))
. (Join-Path $PSScriptRoot 'win-assets/bvagent-input.ps1')
$id = '12345678-1234-1234-1234-123456789abc'
$result = Get-InputCapabilities $id
if ($result.Exit -ne 0 -or $result.Out -cne ('BVINPUT_CAPS ' + $id + ' 2 TEXTINPUT KEYINPUT POINTERINPUT 65536')) {
    throw 'resident capability response mismatch'
}
foreach ($invalid in @('', 'invalid', ($id + ' extra'), ($id + "`n"), ($id + "`r`n"))) {
    if ((Get-InputCapabilities $invalid).Exit -eq 0) { throw 'invalid capability UUID accepted' }
}
if (-not $ast.Extent.Text.Contains("'INPUTCAPS' { Write-CommandResult `$h (Get-InputCapabilities `$arg) }")) {
    throw 'capability wire dispatch missing'
}
function Invoke-UnicodeInput { return @{ Exit = 1; Out = 'BVINPUT_FAILED unavailable' } }
if ((Get-InputCapabilities $id).Exit -eq 0) { throw 'unavailable input module advertised' }
Write-Output 'PASS: resident input capability dry-build contracts (no SendInput)'
