$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'win-assets'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'bvagent.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'guest agent parse failure' }
if (-not $ast.Extent.Text.Contains(". (Join-Path `$PSScriptRoot 'bvagent-input.ps1')")) {
    throw 'resident agent does not load the fixed sibling input module'
}
. (Join-Path $root 'bvagent-input.ps1')
if (-not $ast.Extent.Text.Contains("{ `$_ -in @('TEXTINPUT', 'KEYINPUT', 'POINTERINPUT') } { Write-CommandResult `$h (Invoke-UnicodeInput `$arg `$null (`$tok -eq 'KEYINPUT') (`$tok -eq 'POINTERINPUT')) }")) {
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
& (Join-Path $PSScriptRoot 'test-bvagent-input-assets.ps1')
& (Join-Path $PSScriptRoot 'test-bvagent-key-input.ps1')

& (Join-Path $PSScriptRoot 'test-bvagent-input-capabilities.ps1')
