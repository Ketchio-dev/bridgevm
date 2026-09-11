$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'win-assets'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'bvagent.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'guest agent parse failure' }
$assignment = $ast.Find({ param($node)
    $node -is [Management.Automation.Language.AssignmentStatementAst] -and
    $node.Left.Extent.Text -eq '$script:BvUnicodeInputSource'
}, $true)
if ($null -eq $assignment) { throw 'embedded Unicode source missing' }
. ([scriptblock]::Create($assignment.Extent.Text))
$original = [IO.File]::ReadAllText((Join-Path $root 'bvagent-unicode-input.cs'))
if ($script:BvUnicodeInputSource.Replace("`r`n", "`n").TrimEnd() -cne $original.Replace("`r`n", "`n").TrimEnd()) {
    throw 'embedded Unicode source differs from tested source'
}
$definition = $ast.Find({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-UnicodeInput'
}, $true)
if ($null -eq $definition) { throw 'guest Unicode handler missing' }
. ([scriptblock]::Create($definition.Extent.Text))
if (-not ('BridgeVM.BvUnicodeInput' -as [type])) { Add-Type -TypeDefinition $script:BvUnicodeInputSource }
if (-not $ast.Extent.Text.Contains("'TEXTINPUT' { Write-CommandResult `$h (Invoke-UnicodeInput `$arg) }")) {
    throw 'guest dispatch does not invoke the Unicode handler/result writer'
}
$id = '01234567-89AB-CDEF-0123-456789ABCDEF'
$text = [string][char]0xD55C + [char]::ConvertFromUtf32(0x1F642)
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text))
$script:seen = $null
$sender = { param($value) $script:seen = $value; return [uint32]($value.Length * 2) }
$result = Invoke-UnicodeInput "$id $payload" $sender
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 6" -or $script:seen -cne $text) {
    throw 'valid Unicode request did not preserve text and receipt identity'
}
foreach ($bad in @("invalid $payload", "$id ", "$id !", "$id YQ== extra", "$id /w==", "$id YQ==`n", "$id`n YQ==", "$id YQ==`r`n", "$id`r`n YQ==")) {
    $script:seen = $null
    $result = Invoke-UnicodeInput $bad $sender
    if ($result.Exit -eq 0 -or $null -ne $script:seen) { throw 'invalid input reached sender' }
}
$result = Invoke-UnicodeInput "$id $payload" { param($value) return [uint32]1 }
if ($result.Exit -eq 0 -or $result.Out -notlike "BVINPUT_FAILED $id *") { throw 'partial insertion accepted' }
$result = Invoke-UnicodeInput "$id $payload" { throw 'sender unavailable' }
if ($result.Exit -eq 0 -or $result.Out -notlike "BVINPUT_FAILED $id *") { throw 'sender failure escaped receipt' }
$oversized = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes(('a' * 65537)))
$script:seen = $null
$result = Invoke-UnicodeInput "$id $oversized" $sender
if ($result.Exit -eq 0 -or $null -ne $script:seen) { throw 'oversized request reached sender' }
Write-Output 'Unicode encoding and resident request contracts: PASS (no native input injected)'
