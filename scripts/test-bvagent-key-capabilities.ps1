param([ValidateSet('all', 'current', 'base', 'enter-only', 'space-only')][string]$Mode = 'all')
$ErrorActionPreference = 'Stop'
if ($Mode -eq 'all') {
    foreach ($variant in @('current', 'base', 'enter-only', 'space-only')) {
        & (Get-Process -Id $PID).Path -NoProfile -File $PSCommandPath -Mode $variant
        if ($LASTEXITCODE -ne 0) { throw "key capability variant failed: $variant" }
    }
    Write-Output 'PASS: resident base/expanded key capabilities (no SendInput)'
    return
}
$assets = Join-Path $PSScriptRoot 'win-assets'
$keySource = [IO.File]::ReadAllText((Join-Path $assets 'bvagent-key-input.cs'))
$expanded = ' || key == "enter" || key == "space"'
if (-not $keySource.Contains($expanded)) { throw 'missing expanded key allowlist' }
$replacement = switch ($Mode) {
    'base' { '' }; 'enter-only' { ' || key == "enter"' }; 'space-only' { ' || key == "space"' }; default { $expanded }
}
# Fresh child processes model resident compiled modules; Add-Type cannot replace a loaded type.
$temporary = Join-Path ([IO.Path]::GetTempPath()) (([guid]::NewGuid().ToString()) + '.cs')
try {
    [IO.File]::WriteAllText($temporary, $keySource.Replace($expanded, $replacement))
    Add-Type -Path (Join-Path $assets 'bvagent-unicode-input.cs'),$temporary,(Join-Path $assets 'bvagent-pointer-input.cs')
} finally { Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue }
. (Join-Path $assets 'bvagent-input.ps1')
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $assets 'bvagent.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'resident parse failed' }
$definition = $ast.Find({ param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-InputCapabilities'
}, $true)
if ($null -eq $definition) { throw 'missing resident capability function' }
. ([scriptblock]::Create($definition.Extent.Text))
$id = '12345678-1234-1234-1234-123456789abc'
$expected = "BVINPUT_CAPS $id 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"
if ($Mode -eq 'current') { $expected += "`nBVINPUT_KEYS $id SHIFT_ENTER_SPACE" }
$result = Get-InputCapabilities $id
if ($result.Exit -ne 0 -or $result.Out -cne $expected) { throw "incorrect advertised keyset: $Mode" }
$ordinary = Invoke-UnicodeInput "$id YQ==" { param($text) [uint32]2 }
if ($ordinary.Exit -ne 0 -or $ordinary.Out -cne "BVINPUT_INSERTED $id 2") { throw 'ordinary input was disabled' }
if ([BridgeVM.BvUnicodeInput]::BuildKey('enter').Length -ne 2) { throw 'base key support lost' }
Write-Output "PASS: $Mode resident module advertises only its complete keyset"
