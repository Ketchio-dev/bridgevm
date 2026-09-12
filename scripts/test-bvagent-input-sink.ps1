$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'win-assets/bv-input-order-sink.ps1') -ValidateOnly
function Assert-Proof($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
$state = @{ Nonce = [Guid]::NewGuid().ToString('D'); EnterCount = 1; EnterSawFirstText = $true; FocusLost = $false }
$expected = 'BridgeVM' + "`r`n" + [string][char]0xD55C + [char]0xAE00 + [char]0xD83D + [char]0xDE42
$proof = Get-BvInputProof $state $expected $true
Assert-Proof $proof.passed 'Complete synthetic state should match'
Assert-Proof ($proof.nonce -ceq $state.Nonce) 'Nonce lost'
Assert-Proof ($proof.actual_text_sha256 -match '^[0-9a-f]{64}$') 'Missing content hash'
Assert-Proof (-not $proof.Contains('actual_text')) 'Do not export arbitrary captured text'
foreach ($count in @(0, 2)) {
    $bad = $state.Clone(); $bad.EnterCount = $count
    Assert-Proof (-not (Get-BvInputProof $bad $expected $true).passed) 'Wrong Enter count accepted'
}
$bad = $state.Clone(); $bad.EnterSawFirstText = $false
Assert-Proof (-not (Get-BvInputProof $bad $expected $true).passed) 'Wrong input order accepted'
$bad = $state.Clone(); $bad.FocusLost = $true
Assert-Proof (-not (Get-BvInputProof $bad $expected $true).passed) 'Focus loss accepted'
Assert-Proof (-not (Get-BvInputProof $state $expected $false).passed) 'Missing application click accepted'
Assert-Proof (-not (Get-BvInputProof $state ('wrong' + $expected) $true).passed) 'Wrong text accepted'
Assert-Proof (-not (Get-BvInputProof $state "BridgeVM`r`n" $true).passed) 'Missing Unicode accepted'
Write-Output 'PASS: synthetic input-sink state checks and native declaration compilation; no GUI input or VM proven'
exit 0
