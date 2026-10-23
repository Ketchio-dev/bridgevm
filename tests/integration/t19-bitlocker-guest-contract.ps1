# Contract: the T19 BitLocker guest workload and its launcher parse, and the workload's
# enable and status steps, run against scripted Win32_EncryptableVolume, registry, Get-Tpm
# and TBS answers, take exactly the documented calls and refusals, pass only the
# documented arguments, and format only the documented 12-line result of at most 512
# bytes. With -Emit it prints each scenario's result for the independent parser in
# t19-bitlocker-guest-contract.py. On Windows it first reads this machine's own BitLocker
# status, read-only, and prints only the outcome code.
param([switch]$Emit)
$ErrorActionPreference = 'Stop'
$Root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$Assets = Join-Path $Root 'scripts/win-assets'
foreach ($Name in 'bv-product-e2e-bitlocker.ps1', 'bv-product-e2e-bitlocker-launch.ps1') {
    $Tokens = $null; $Errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile((Join-Path $Assets $Name), [ref]$Tokens, [ref]$Errors)
    if ($Errors.Count -ne 0) { throw "$Name does not parse: $($Errors | Out-String)" }
}
$Tokens = $null; $Errors = $null
$Ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $Assets 'bv-product-e2e-bitlocker.ps1'), [ref]$Tokens, [ref]$Errors)
$Definitions = @($Ast.FindAll({ param($Node) $Node -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $false))
if ($Definitions.Count -eq 0) { throw 'the workload defines no functions' }
foreach ($Definition in $Definitions) { . ([scriptblock]::Create($Definition.Extent.Text)) }

# The real status read changes nothing; a hosted runner may lack BitLocker, so any fixed outcome passes.
if ($PSVersionTable.PSEdition -eq 'Desktop' -or $IsWindows) {
    $Real = Invoke-BvStatus
    if ($Real.Outcome -notin @('ok', 'bitlocker-unavailable', 'query-failed')) { throw "real status read returned $($Real.Outcome)" }
    Write-Output "T19 BitLocker guest contract: read-only status outcome=$($Real.Outcome)"
}

$Nonce = 'c3' * 32; $Pcr0 = '0a' * 32; $Pcr7 = '7b' * 32
$Allowed = @('GetConversionStatus', 'GetProtectionStatus', 'GetLockStatus', 'GetKeyProtectors', 'GetKeyProtectorType',
    'GetKeyProtectorPlatformValidationProfile', 'ProtectKeyWithTPM', 'Encrypt')
$ExpectedShapes = @{ GetConversionStatus = 'PrecisionFactor=0'; GetProtectionStatus = ''; GetLockStatus = ''; GetKeyProtectors = 'KeyProtectorType=0'
    GetKeyProtectorType = 'VolumeKeyProtectorID'; GetKeyProtectorPlatformValidationProfile = 'VolumeKeyProtectorID'
    ProtectKeyWithTPM = 'FriendlyName'; Encrypt = 'EncryptionFlags=1' }
$script:Calls = [System.Collections.Generic.List[string]]::new()

function Set-Fake([hashtable]$Settings) {
    $script:Calls.Clear()
    $script:Fake = @{ Edition = 'Professional'; TpmReady = $true; TpmThrows = $false; Volume = $true; Conversion = 0; Percent = 0
        Protection = 0; Lock = 0; Existing = @(); Added = @(1); Profile = @(7, 11); Protect = 0; Encrypt = 0; EncryptThrows = $false; Fail = '' }
    foreach ($Key in $Settings.Keys) { $script:Fake[$Key] = $Settings[$Key] }
    $script:Ids = [ordered]@{}
    foreach ($Type in $script:Fake.Existing) { Add-FakeProtector $Type }
}
function Add-FakeProtector([int]$Type) { $script:Ids["{$($script:Ids.Count)}"] = $Type }
function Get-BvEditionId { $script:Fake.Edition }
function Get-BvTpmReady { if ($script:Fake.TpmThrows) { throw 'Get-Tpm failed' }; $script:Fake.TpmReady }
function Get-BvVolume { if ($script:Fake.Volume) { [pscustomobject]@{ DriveLetter = 'C:' } } }
function Read-BvPcr([string]$Share) { @{ Status = 'ok'; Pcr0 = $Pcr0; Pcr7 = $Pcr7 } }
function Invoke-BvMethod($Volume, [string]$Method, [hashtable]$Arguments = @{}) {
    $script:Calls.Add($Method)
    if ($Method -notin $Allowed) { throw "method outside the allowed set: $Method" }
    $Shape = @($Arguments.Keys | Sort-Object | ForEach-Object { if ($_ -eq 'VolumeKeyProtectorID' -or $_ -eq 'FriendlyName') { $_ } else { "$_=$($Arguments[$_])" } }) -join ','
    if ($Shape -ne $ExpectedShapes[$Method]) { throw "$Method arguments '$Shape'" }
    $F = $script:Fake
    if ($F.Fail -eq $Method) { return [pscustomobject]@{ ReturnValue = [uint32]2147749889 } }
    switch ($Method) {
        'GetConversionStatus' { return [pscustomobject]@{ ReturnValue = [uint32]0; ConversionStatus = [uint32]$F.Conversion; EncryptionPercentage = [uint32]$F.Percent } }
        'GetProtectionStatus' { return [pscustomobject]@{ ReturnValue = [uint32]0; ProtectionStatus = [uint32]$F.Protection } }
        'GetLockStatus' { return [pscustomobject]@{ ReturnValue = [uint32]0; LockStatus = [uint32]$F.Lock } }
        'GetKeyProtectors' { return [pscustomobject]@{ ReturnValue = [uint32]0; VolumeKeyProtectorID = [string[]]@($script:Ids.Keys) } }
        'GetKeyProtectorType' { return [pscustomobject]@{ ReturnValue = [uint32]0; KeyProtectorType = [uint32]$script:Ids[$Arguments.VolumeKeyProtectorID] } }
        'GetKeyProtectorPlatformValidationProfile' { return [pscustomobject]@{ ReturnValue = [uint32]0; PlatformValidationProfile = [byte[]]$F.Profile } }
        'ProtectKeyWithTPM' {
            if ($F.Protect -eq 0) { foreach ($Type in $F.Added) { Add-FakeProtector $Type } }
            return [pscustomobject]@{ ReturnValue = [uint32]$F.Protect; VolumeKeyProtectorID = '{added}' }
        }
        'Encrypt' {
            if ($F.EncryptThrows) { throw 'CIM call failed' }
            if ($F.Encrypt -eq 0) { $F.Conversion = 2 }
            return [pscustomobject]@{ ReturnValue = [uint32]$F.Encrypt }
        }
    }
}

$Read = @('GetConversionStatus', 'GetProtectionStatus', 'GetLockStatus', 'GetKeyProtectors')
function Test-Scenario([string]$Name, [string]$Phase, [hashtable]$Settings, [string]$Expected, [string[]]$Calls) {
    Set-Fake $Settings
    $Result = if ($Phase -eq 'enable') { Invoke-BvEnable } else { Invoke-BvStatus }
    $Body = Format-BvResult $Nonce $Phase $Result (Read-BvPcr '')
    $Lines = $Body.Split("`n")
    if ($Lines.Count -ne 13 -or $Lines[12] -ne '' -or [Text.Encoding]::UTF8.GetByteCount($Body) -gt 512) { throw "$Name result is not 12 bounded lines" }
    $Observed = ($Lines[3..8] | ForEach-Object { $_.Substring($_.IndexOf('=') + 1) }) -join ' '
    if ($Observed -ne $Expected) { throw "$Name gave '$Observed', expected '$Expected'" }
    $Fixed = "bridgevm-t19-bitlocker-v1|nonce=$Nonce|phase=$Phase|pcr_read=ok|pcr0_sha256=$Pcr0|pcr7_sha256=$Pcr7"
    if ((@($Lines[0..2] + $Lines[9..11]) -join '|') -ne $Fixed) { throw "$Name fixed lines differ" }
    if (($script:Calls -join ',') -ne ($Calls -join ',')) { throw "$Name called $($script:Calls -join ',')" }
    if ($Phase -eq 'status' -and ($script:Calls -contains 'ProtectKeyWithTPM' -or $script:Calls -contains 'Encrypt')) { throw "$Name changed the volume while reading" }
    if ($Emit) { Write-Output (ConvertTo-Json -Compress @{ name = $Name; body = $Body }) }
}

$Tpm = $Read + @('GetKeyProtectorType', 'GetKeyProtectorPlatformValidationProfile')
Test-Scenario 'enabled' enable @{} 'ok pro 2,0,0,0 1 7,11 none' ($Read + 'ProtectKeyWithTPM' + $Tpm + 'Encrypt' + $Tpm)
Test-Scenario 'home edition' enable @{ Edition = 'CoreSingleLanguage' } 'edition-unsupported home unknown unknown unknown none' @()
Test-Scenario 'tpm not ready' enable @{ TpmReady = $false } 'tpm-not-ready pro unknown unknown unknown none' @()
Test-Scenario 'get-tpm failed' enable @{ TpmThrows = $true } 'tpm-not-ready pro unknown unknown unknown none' @()
Test-Scenario 'no volume' enable @{ Volume = $false } 'bitlocker-unavailable pro unknown unknown unknown none' @()
Test-Scenario 'device encryption with a recovery protector' enable @{ Conversion = 1; Percent = 100; Existing = @(3) } 'volume-not-decrypted pro 1,0,0,100 3 none none' ($Read + 'GetKeyProtectorType')
Test-Scenario 'protector present' enable @{ Existing = @(1) } 'protectors-present pro 0,0,0,0 1 7,11 none' $Tpm
Test-Scenario 'protector refused' enable @{ Protect = 2150694960 } 'protector-add-failed pro 0,0,0,0 none none 48' ($Read + 'ProtectKeyWithTPM' + $Read)
Test-Scenario 'recovery protector added by policy' enable @{ Added = @(1, 3) } 'protector-unexpected pro 0,0,0,0 1,3 7,11 none' ($Read + 'ProtectKeyWithTPM' + $Read + 'GetKeyProtectorType' + $Tpm[-2..-1])
Test-Scenario 'encryption refused' enable @{ Encrypt = 2150695073 } 'encrypt-failed pro 0,0,0,0 1 7,11 161' ($Read + 'ProtectKeyWithTPM' + $Tpm + 'Encrypt' + $Tpm)
Test-Scenario 'encryption failed outside FVE' enable @{ Encrypt = 2147749889 } 'encrypt-failed pro 0,0,0,0 1 7,11 none' ($Read + 'ProtectKeyWithTPM' + $Tpm + 'Encrypt' + $Tpm)
Test-Scenario 'encryption threw' enable @{ EncryptThrows = $true } 'encrypt-failed pro 0,0,0,0 1 7,11 none' ($Read + 'ProtectKeyWithTPM' + $Tpm + 'Encrypt' + $Tpm)
Test-Scenario 'protector query failed' enable @{ Fail = 'GetKeyProtectors' } 'query-failed pro unknown unknown unknown none' $Read
Test-Scenario 'conversion out of range' enable @{ Conversion = 7 } 'query-failed pro unknown unknown unknown none' $Read
Test-Scenario 'unknown edition' enable @{ Edition = $null } 'ok unknown 2,0,0,0 1 7,11 none' ($Read + 'ProtectKeyWithTPM' + $Tpm + 'Encrypt' + $Tpm)
Test-Scenario 'unlocked under conversion' status @{ Conversion = 2; Percent = 37; Existing = @(1); Profile = @(11, 4, 2, 0, 4) } 'ok pro 2,0,0,37 1 0,2,4,11 none' $Tpm
Test-Scenario 'unlocked and encrypted' status @{ Edition = 'EnterpriseS'; Conversion = 1; Percent = 100; Protection = 1; Existing = @(1) } 'ok enterprise 1,1,0,100 1 7,11 none' $Tpm
Test-Scenario 'status without volume' status @{ Edition = 'Education'; Volume = $false } 'bitlocker-unavailable education unknown unknown unknown none' @()
Test-Scenario 'status query failed' status @{ Edition = 'ServerDatacenter'; Fail = 'GetLockStatus' } 'query-failed server unknown unknown unknown none' $Read

foreach ($Case in @(@('Core', 'home'), @('ProfessionalWorkstation', 'pro'), @('IoTEnterprise', 'other'), @('', 'unknown'))) {
    if ((ConvertTo-BvEdition $Case[0]) -ne $Case[1]) { throw "edition $($Case[0]) is not $($Case[1])" }
}
$Longest = @{ Outcome = 'bitlocker-unavailable'; Edition = 'enterprise'; Conversion = 5; Protection = 2; Lock = 1; Percent = 100
    Protectors = (0..10) -join ','; Profile = (0..23) -join ','; Fve = '65535' }
$Body = Format-BvResult $Nonce 'status' $Longest @{ Status = 'ok'; Pcr0 = $Pcr0; Pcr7 = $Pcr7 }
if ([Text.Encoding]::UTF8.GetByteCount($Body) -gt 512 -or $Body.Split("`n").Count -ne 13) { throw 'the longest result exceeds its bound' }
if ($Emit) { Write-Output (ConvertTo-Json -Compress @{ name = 'longest'; body = $Body }) }
Write-Output 'T19 BitLocker guest contract: PASS'
exit 0
