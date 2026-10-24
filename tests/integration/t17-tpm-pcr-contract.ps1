# Contract: the T17 TPM workload and its launcher parse, and the TBS helper compiles as
# C# 5 (Windows PowerShell 5.1's Add-Type), builds exactly the documented TPM2_PCR_Read
# command and accepts only that command's exact success response, returning SHA-256
# hashes of PCR 0 and PCR 7 and never the values. With -ResponseHex it prints one parse
# for a caller that builds responses independently (t17-tpm-pcr-command-contract.py).
param([string]$ResponseHex = '')
$ErrorActionPreference = 'Stop'
$Root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$Assets = Join-Path $Root 'scripts/win-assets'
$Source = Join-Path $Assets 'bv-product-e2e-tpm.cs'
if ($PSVersionTable.PSEdition -eq 'Core') { Add-Type -Path $Source -CompilerOptions '/langversion:5' } else { Add-Type -Path $Source }
function Bytes([string]$Hex) { [byte[]]@(for ($Index = 0; $Index -lt $Hex.Length; $Index += 2) { [Convert]::ToByte($Hex.Substring($Index, 2), 16) }) }
function Hex([byte[]]$Data) { -join @($Data | ForEach-Object { $_.ToString('x2') }) }
function Parse([string]$Hex) { $Data = Bytes $Hex; [BridgeVM.BvTpmPcr]::Parse($Data, $Data.Length) }
if ($ResponseHex) { $Reading = Parse $ResponseHex; Write-Output "$($Reading.Status) $($Reading.Pcr0Sha256) $($Reading.Pcr7Sha256)"; exit 0 }

foreach ($Name in 'bv-product-e2e-tpm.ps1', 'bv-product-e2e-launch.ps1') {
    $Tokens = $null; $Errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile((Join-Path $Assets $Name), [ref]$Tokens, [ref]$Errors)
    if ($Errors.Count -ne 0) { throw "$Name does not parse: $($Errors | Out-String)" }
}
$Command = '8001' + '00000014' + '0000017e' + '00000001' + '000b' + '03' + '810000'
if ((Hex ([BridgeVM.BvTpmPcr]::PcrReadCommand())) -ne $Command) { throw 'TPM2_PCR_Read command bytes changed' }
$Copy = [BridgeVM.BvTpmPcr]::PcrReadCommand(); $Copy[0] = 0
if ((Hex ([BridgeVM.BvTpmPcr]::PcrReadCommand())) -ne $Command) { throw 'callers can mutate the shared command' }

$Pcr0 = -join @(0..31 | ForEach-Object { $_.ToString('x2') }); $Pcr7 = -join @(160..191 | ForEach-Object { $_.ToString('x2') })
$Sha = [Security.Cryptography.SHA256]::Create()
$Pcr0Sha = Hex ($Sha.ComputeHash((Bytes $Pcr0))); $Pcr7Sha = Hex ($Sha.ComputeHash((Bytes $Pcr7)))
$Selection = '00000001' + '000b' + '03' + '810000'
$Success = '8001' + '00000060' + '00000000' + '0000002a' + $Selection + '00000002' + '0020' + $Pcr0 + '0020' + $Pcr7
$Reading = Parse $Success
if ($Reading.Status -ne 'ok' -or $Reading.Pcr0Sha256 -ne $Pcr0Sha -or $Reading.Pcr7Sha256 -ne $Pcr7Sha) { throw "success response parsed as $($Reading.Status)" }
if ("$($Reading.Pcr0Sha256)$($Reading.Pcr7Sha256)".Contains($Pcr0) -or "$($Reading.Pcr0Sha256)$($Reading.Pcr7Sha256)".Contains($Pcr7)) { throw 'a PCR value left the helper' }
$Buffer = New-Object byte[] 4096; [Array]::Copy((Bytes $Success), $Buffer, 96)
if ([BridgeVM.BvTpmPcr]::Parse($Buffer, 96).Status -ne 'ok') { throw 'a success response in the TBS buffer was refused' }

# Keys: TPM_RC_FAILURE, TPM_RC_INITIALIZE, an unallocated SHA-256 bank, the SHA-1 bank,
# a 95-byte response, a size field of 97, a 31-byte digest, a session tag, 9 bytes.
$Cases = [ordered]@{
    ('8001' + '0000000a' + '00000101') = 'tpm-error-response'
    ('8001' + '0000000a' + '00000100') = 'tpm-error-response'
    ('8001' + '0000001c' + '00000000' + '0000002a' + '00000001' + '000b' + '03' + '000000' + '00000000') = 'pcr-selection-mismatch'
    ('8001' + '00000060' + '00000000' + '0000002a' + '00000001' + '0004' + '03' + '810000' + '00000002' + '0020' + $Pcr0 + '0020' + $Pcr7) = 'pcr-selection-mismatch'
    ('8001' + '0000005f' + $Success.Substring(12, 178)) = 'pcr-response-malformed'
    ('8001' + '00000061' + $Success.Substring(12)) = 'pcr-response-malformed'
    ($Success.Substring(0, 56) + '001f' + $Success.Substring(60)) = 'pcr-response-malformed'
    ('8002' + $Success.Substring(4)) = 'pcr-response-malformed'
    ('8001' + '00000009' + '000000') = 'pcr-response-malformed'
}
foreach ($Case in $Cases.GetEnumerator()) {
    $Reading = Parse $Case.Key
    if ($Reading.Status -ne $Case.Value -or $Reading.Pcr0Sha256 -ne 'absent' -or $Reading.Pcr7Sha256 -ne 'absent') { throw "response $($Case.Key) parsed as $($Reading.Status), expected $($Case.Value)" }
}
if ([BridgeVM.BvTpmPcr]::Parse($null, 0).Status -ne 'pcr-response-malformed') { throw 'a null response was accepted' }
if ([BridgeVM.BvTpmPcr]::Parse((Bytes $Success), 97).Status -ne 'pcr-response-malformed') { throw 'a length beyond the buffer was accepted' }

# On Windows the TBS entry points must bind; a hosted runner may have no TPM, so only the
# fixed status set is required and nothing it returns is printed.
if ($PSVersionTable.PSEdition -eq 'Desktop' -or $IsWindows) {
    $Live = [BridgeVM.BvTpmPcr]::Read()
    if ($Live.Status -notin @('ok', 'tbs-context-failed', 'tbs-submit-failed', 'tpm-error-response', 'pcr-selection-mismatch', 'pcr-response-malformed')) { throw 'TBS read returned an unknown status' }
    Write-Output "T17 TPM PCR contract: TBS bound, status=$($Live.Status)"
}
Write-Output 'T17 TPM PCR contract: PASS'
exit 0
