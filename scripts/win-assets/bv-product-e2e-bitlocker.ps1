[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('BitLockerEnable', 'BitLockerStatus')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$Nonce
)
# The guest side of the T19 BitLocker restart check
# (docs/history/windows-hvf/bitlocker-restart-check-20261023.md). BitLockerEnable adds one
# TPM-only protector with the Windows default PCR profile to the system drive and starts a
# used-space conversion without the hardware test; BitLockerStatus only reads. Both reach
# Win32_EncryptableVolume through CIM alone, call only the methods below, and stop at the first
# refusal: no other protector is ever added, read or backed up. Only fixed codes, small integers
# and SHA-256 hashes of PCR 0 and 7 reach the share, in one result of at most 512 bytes staged
# outside the share and renamed whole. A refusal is a recorded code, never the shared error file.
$ErrorActionPreference = 'Stop'

function Get-BvEditionId { (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -Name EditionID).EditionID }
function Get-BvTpmReady { $Tpm = Get-Tpm; ($Tpm.TpmPresent -eq $true) -and ($Tpm.TpmReady -eq $true) }
function Get-BvVolume { Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume -Filter "DriveLetter='$env:SystemDrive'" }
function Invoke-BvMethod($Volume, [string]$Method, [hashtable]$Arguments = @{}) {
    if ($Arguments.Count -eq 0) { return Invoke-CimMethod -InputObject $Volume -MethodName $Method }
    Invoke-CimMethod -InputObject $Volume -MethodName $Method -Arguments $Arguments
}
function Read-BvPcr([string]$Share) {
    try {
        Add-Type -Path (Join-Path $Share 'bv-product-e2e-tpm.cs')
        $Reading = [BridgeVM.BvTpmPcr]::Read()
        @{ Status = $Reading.Status; Pcr0 = $Reading.Pcr0Sha256; Pcr7 = $Reading.Pcr7Sha256 }
    } catch { @{ Status = 'helper-failed'; Pcr0 = 'absent'; Pcr7 = 'absent' } }
}

function ConvertTo-BvEdition([string]$EditionId) {
    switch -Regex ($EditionId) {
        '^Core' { return 'home' }
        '^Professional' { return 'pro' }
        '^Enterprise' { return 'enterprise' }
        '^Education' { return 'education' }
        '^Server' { return 'server' }
        '.' { return 'other' }
    }
    'unknown'
}
function Get-BvSafeEditionId { try { Get-BvEditionId } catch { '' } }
function Find-BvVolume {
    try { $Found = @(Get-BvVolume) } catch { return $null }
    if ($Found.Count -eq 1 -and $null -ne $Found[0]) { return $Found[0] }
    $null
}
function New-BvResult {
    @{ Outcome = 'ok'; Edition = 'unknown'; Conversion = $null; Protection = $null; Lock = $null; Percent = $null
        Protectors = 'unknown'; Profile = 'unknown'; Fve = 'none' }
}
# Distinct values in ascending order, each within 0..Maximum, or none.
function Join-BvList($Values, [int]$Maximum) {
    $Numbers = @($Values | Where-Object { $null -ne $_ } | ForEach-Object { [int]$_ } | Sort-Object -Unique)
    foreach ($Number in $Numbers) { if ($Number -lt 0 -or $Number -gt $Maximum) { throw "list value $Number is out of range" } }
    if ($Numbers.Count -eq 0) { return 'none' }
    $Numbers -join ','
}
function Test-BvAnswer($Answer) { $null -ne $Answer -and $null -ne $Answer.ReturnValue -and [long]$Answer.ReturnValue -eq 0 }
# The low 16 bits of an FVE error (0x8031xxxx) returned by a failing method, else none.
function Get-BvFveCode($Answer) {
    if ($null -eq $Answer -or $null -eq $Answer.ReturnValue) { return 'none' }
    $Code = [long]$Answer.ReturnValue
    if ($Code -lt 0) { $Code += 4294967296 }
    if (($Code -shr 16) -eq 0x8031) { return [string]($Code -band 0xFFFF) }
    'none'
}

# Conversion, protection, lock and percentage, the protector types and the TPM protector's
# PCR profile. The result changes only when every query answered within range.
function Read-BvState($Volume, $Result) {
    $Conversion = Invoke-BvMethod $Volume 'GetConversionStatus' @{ PrecisionFactor = [uint32]0 }
    $Protection = Invoke-BvMethod $Volume 'GetProtectionStatus'
    $Lock = Invoke-BvMethod $Volume 'GetLockStatus'
    $Ids = Invoke-BvMethod $Volume 'GetKeyProtectors' @{ KeyProtectorType = [uint32]0 }
    foreach ($Answer in $Conversion, $Protection, $Lock, $Ids) { if (-not (Test-BvAnswer $Answer)) { throw 'a status method failed' } }
    $Types = @(); $Tpm = @()
    foreach ($Id in @($Ids.VolumeKeyProtectorID | Where-Object { $_ })) {
        $Type = Invoke-BvMethod $Volume 'GetKeyProtectorType' @{ VolumeKeyProtectorID = [string]$Id }
        if (-not (Test-BvAnswer $Type) -or $null -eq $Type.KeyProtectorType) { throw 'a protector type query failed' }
        $Types += [int]$Type.KeyProtectorType
        if ([int]$Type.KeyProtectorType -eq 1) { $Tpm += [string]$Id }
    }
    $Profile = 'none'
    if ($Tpm.Count -eq 1) {
        $Answer = Invoke-BvMethod $Volume 'GetKeyProtectorPlatformValidationProfile' @{ VolumeKeyProtectorID = $Tpm[0] }
        if (-not (Test-BvAnswer $Answer)) { throw 'the profile query failed' }
        $Profile = Join-BvList @($Answer.PlatformValidationProfile) 23
    }
    $Raw = @($Conversion.ConversionStatus, $Protection.ProtectionStatus, $Lock.LockStatus, $Conversion.EncryptionPercentage)
    $Limits = @(5, 2, 1, 100)
    for ($Index = 0; $Index -lt 4; $Index++) {
        if ($null -eq $Raw[$Index] -or [long]$Raw[$Index] -lt 0 -or [long]$Raw[$Index] -gt $Limits[$Index]) { throw 'a status value is missing or out of range' }
    }
    $Protectors = Join-BvList $Types 10
    $Result.Conversion = [int]$Raw[0]; $Result.Protection = [int]$Raw[1]; $Result.Lock = [int]$Raw[2]; $Result.Percent = [int]$Raw[3]
    $Result.Protectors = $Protectors; $Result.Profile = $Profile
}

function Invoke-BvEnable {
    $Result = New-BvResult
    $Result.Edition = ConvertTo-BvEdition (Get-BvSafeEditionId)
    if ($Result.Edition -eq 'home') { $Result.Outcome = 'edition-unsupported'; return $Result }
    $Ready = $false
    try { $Ready = [bool](Get-BvTpmReady) } catch { $Ready = $false }
    if (-not $Ready) { $Result.Outcome = 'tpm-not-ready'; return $Result }
    $Volume = Find-BvVolume
    if ($null -eq $Volume) { $Result.Outcome = 'bitlocker-unavailable'; return $Result }
    try { Read-BvState $Volume $Result } catch { $Result.Outcome = 'query-failed'; return $Result }
    # Only a fully decrypted volume without any protector is changed, so no other protector exists.
    if ($Result.Conversion -ne 0) { $Result.Outcome = 'volume-not-decrypted'; return $Result }
    if ($Result.Protectors -ne 'none') { $Result.Outcome = 'protectors-present'; return $Result }
    $Answer = $null
    try { $Answer = Invoke-BvMethod $Volume 'ProtectKeyWithTPM' @{ FriendlyName = 'BridgeVM T19 BitLocker restart check' } } catch { $Answer = $null }
    if (-not (Test-BvAnswer $Answer)) {
        $Result.Outcome = 'protector-add-failed'; $Result.Fve = Get-BvFveCode $Answer
        try { Read-BvState $Volume $Result } catch { }
        return $Result
    }
    try { Read-BvState $Volume $Result } catch { $Result.Outcome = 'query-failed'; return $Result }
    if ($Result.Protectors -ne '1') { $Result.Outcome = 'protector-unexpected'; return $Result }
    # Encrypt, not the hardware-test variant, with data-only encryption: used space, no test reboot.
    $Answer = $null
    try { $Answer = Invoke-BvMethod $Volume 'Encrypt' @{ EncryptionFlags = [uint32]1 } } catch { $Answer = $null }
    if (-not (Test-BvAnswer $Answer)) { $Result.Outcome = 'encrypt-failed'; $Result.Fve = Get-BvFveCode $Answer }
    try { Read-BvState $Volume $Result } catch { if ($Result.Outcome -eq 'ok') { $Result.Outcome = 'query-failed' } }
    $Result
}

function Invoke-BvStatus {
    $Result = New-BvResult
    $Result.Edition = ConvertTo-BvEdition (Get-BvSafeEditionId)
    $Volume = Find-BvVolume
    if ($null -eq $Volume) { $Result.Outcome = 'bitlocker-unavailable'; return $Result }
    try { Read-BvState $Volume $Result } catch { $Result.Outcome = 'query-failed' }
    $Result
}

function Format-BvResult([string]$Nonce, [string]$Phase, $Result, $Pcr) {
    $Volume = 'unknown'
    if ($null -ne $Result.Conversion) { $Volume = "$($Result.Conversion),$($Result.Protection),$($Result.Lock),$($Result.Percent)" }
    "bridgevm-t19-bitlocker-v1`nnonce=$Nonce`nphase=$Phase`noutcome=$($Result.Outcome)`nedition=$($Result.Edition)`nvolume=$Volume`nprotectors=$($Result.Protectors)`npcr_profile=$($Result.Profile)`nfve_code=$($Result.Fve)`npcr_read=$($Pcr.Status)`npcr0_sha256=$($Pcr.Pcr0)`npcr7_sha256=$($Pcr.Pcr7)`n"
}

$Share = 'C:\bridgevm-share'; $Prefix = $Nonce.Substring(0, 12)
$Phase = 'status'; if ($Action -eq 'BitLockerEnable') { $Phase = 'enable' }
try { if ($Phase -eq 'enable') { $Result = Invoke-BvEnable } else { $Result = Invoke-BvStatus } } catch { $Result = New-BvResult; $Result.Outcome = 'script-failed' }
$Body = Format-BvResult $Nonce $Phase $Result (Read-BvPcr $Share)
New-Item -ItemType Directory -Force -Path 'C:\ProgramData\BridgeVM' | Out-Null
$Staged = "C:\ProgramData\BridgeVM\t19-bitlocker-$Phase.tmp"
[IO.File]::WriteAllBytes($Staged, (New-Object System.Text.UTF8Encoding($false)).GetBytes($Body))
Move-Item -LiteralPath $Staged -Destination (Join-Path $Share "t19-bitlocker-$Phase-$Prefix.txt") -Force
Write-Output ("T19 action=$Action nonce=$Nonce status=PASS")
exit 0
