param([string]$Python = 'python3')
$ErrorActionPreference = 'Stop'
# Execute the real copied Query/Launch script; all CIM calls are controlled mocks.
$source = Join-Path $PSScriptRoot '../../scripts/win-assets/bv-t22-pair-admission.ps1'
$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
$root = Join-Path ([IO.Path]::GetTempPath()) ('bridgevm-t22-pair-contract-' + [Guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $root)
$previousDrive = $env:SystemDrive
if (Get-Variable -Scope Global -Name T22AdmissionFixture -ErrorAction SilentlyContinue) { throw 'refusing an existing mock fixture' }
$global:T22AdmissionFixture = @{ passed = 0 }
function Assert([bool]$Value, [string]$Message) { if (-not $Value) { throw $Message } }
function Assert-Keys($Value, [string[]]$Keys) {
    Assert ((@($Value.PSObject.Properties.Name | Sort-Object) -join '|') -ceq (($Keys | Sort-Object) -join '|')) 'wrong JSON field set'
}
function Get-CimInstance {
    [CmdletBinding()] param([string]$ClassName, [string]$Namespace, [string]$Filter)
    $global:T22AdmissionFixture.calls++
    if ($global:T22AdmissionFixture.providerError) { throw 'synthetic provider failure' }
    if ($ClassName -ceq 'Win32_Volume' -and $Filter -ceq 'DriveType=3' -and -not $Namespace) { return $global:T22AdmissionFixture.inventory }
    if ($ClassName -ceq 'Win32_EncryptableVolume' -and $Namespace -ceq 'root/CIMV2/Security/MicrosoftVolumeEncryption' -and -not $Filter) { return $global:T22AdmissionFixture.providers }
    throw 'unexpected CIM query (real providers are forbidden)'
}
function Invoke-CimMethod {
    [CmdletBinding()] param($InputObject, [string]$ClassName, [string]$MethodName, [hashtable]$Arguments)
    $global:T22AdmissionFixture.calls++
    if ($global:T22AdmissionFixture.methodError) { throw 'synthetic method failure' }
    if ($ClassName -ceq 'Win32_Process' -and $MethodName -ceq 'Create') {
        $global:T22AdmissionFixture.launchCommand = $Arguments.CommandLine
        return $global:T22AdmissionFixture.created
    }
    Assert (-not $ClassName -and $null -ne $InputObject) 'unexpected method owner'
    if ($MethodName -ceq 'GetConversionStatus') {
        Assert ($Arguments.Count -eq 1 -and $Arguments.PrecisionFactor -is [uint32] -and $Arguments.PrecisionFactor -eq 0) 'wrong conversion arguments'
    } else { Assert ($Arguments.Count -eq 0) 'unexpected method arguments' }
    Assert ($global:T22AdmissionFixture.methods.ContainsKey($MethodName)) 'unexpected encryption method'
    $global:T22AdmissionFixture.methodOwners += ($InputObject.DeviceID + '|' + $MethodName)
    if ($global:T22AdmissionFixture.overrides.ContainsKey($InputObject.DeviceID) -and $global:T22AdmissionFixture.overrides[$InputObject.DeviceID].ContainsKey($MethodName)) {
        return [pscustomobject]$global:T22AdmissionFixture.overrides[$InputObject.DeviceID][$MethodName]
    }
    return [pscustomobject]$global:T22AdmissionFixture.methods[$MethodName]
}
function Reset-Fixture {
    $env:SystemDrive = 'C:'
    $global:T22AdmissionFixture.ids = @{
        os='\\?\Volume{00000000-0000-0000-0000-000000000001}\'
        data='\\?\Volume{00000000-0000-0000-0000-000000000002}\'
        efi='\\?\Volume{00000000-0000-0000-0000-000000000003}\'
        fat='\\?\Volume{00000000-0000-0000-0000-000000000004}\'
        extra='\\?\Volume{00000000-0000-0000-0000-000000000005}\'
    }
    $global:T22AdmissionFixture.methodOwners = @(); $global:T22AdmissionFixture.overrides = @{}
    $global:T22AdmissionFixture.calls = 0; $global:T22AdmissionFixture.providerError = $false; $global:T22AdmissionFixture.methodError = $false; $global:T22AdmissionFixture.launchCommand = ''
    $global:T22AdmissionFixture.inventory = @(
        [pscustomobject]@{DeviceID=$global:T22AdmissionFixture.ids.os; DriveLetter='C:'; FileSystem='NTFS'; DriveType=[uint32]3},
        [pscustomobject]@{DeviceID=$global:T22AdmissionFixture.ids.data; DriveLetter=$null; FileSystem='NTFS'; DriveType=[uint32]3},
        [pscustomobject]@{DeviceID=$global:T22AdmissionFixture.ids.efi; DriveLetter=$null; FileSystem='FAT32'; DriveType=[uint32]3},
        [pscustomobject]@{DeviceID=$global:T22AdmissionFixture.ids.fat; DriveLetter='D:'; FileSystem='FAT'; DriveType=[int]3}
    )
    $global:T22AdmissionFixture.providers = @([pscustomobject]@{DeviceID=$global:T22AdmissionFixture.ids.os}, [pscustomobject]@{DeviceID=$global:T22AdmissionFixture.ids.data})
    $global:T22AdmissionFixture.methods = @{
        GetConversionStatus=@{ReturnValue=[uint32]0; ConversionStatus=[uint32]0}
        GetEncryptionMethod=@{ReturnValue=[uint32]0; EncryptionMethod=[uint32]0}
        GetProtectionStatus=@{ReturnValue=[uint32]0; ProtectionStatus=[uint32]0}
    }
    $global:T22AdmissionFixture.created = [pscustomobject]@{ReturnValue=[uint32]0; ProcessId=[uint32]123}
    $global:T22AdmissionFixture.nonce = 'a' * 32; $global:T22AdmissionFixture.expected = $sourceHash; $global:T22AdmissionFixture.action = 'Query'
    $global:T22AdmissionFixture.collision = ''; $global:T22AdmissionFixture.noCalls = $false
}
function Run-Case([string]$Name, [scriptblock]$Change, [bool]$Accept) {
    Reset-Fixture
    $directory = Join-Path $root ('case-' + $global:T22AdmissionFixture.passed)
    [void](New-Item -ItemType Directory -Path $directory)
    $probe = Join-Path $directory 'bv-t22-pair-admission.ps1'
    Copy-Item -LiteralPath $source -Destination $probe
    & $Change
    $artifactNonce = if ($global:T22AdmissionFixture.nonce -cmatch '^[0-9a-f]{32}\z') { $global:T22AdmissionFixture.nonce } else { 'a' * 32 }; $json = Join-Path $directory ('result-' + $artifactNonce + '.json')
    $done = Join-Path $directory ('result-' + $artifactNonce + '.done')
    $reserved = if ($global:T22AdmissionFixture.collision) { Join-Path $directory ('result-' + $artifactNonce + $global:T22AdmissionFixture.collision) } else { $null }
    if ($reserved) { [IO.File]::WriteAllText($reserved, 'owned collision') }
    $accepted = $false; $output = @(); $failure = ''
    try { $output = @(& $probe -Action $global:T22AdmissionFixture.action -Nonce $global:T22AdmissionFixture.nonce -ExpectedSha256 $global:T22AdmissionFixture.expected); $accepted = $true }
    catch { $failure = $_.Exception.Message }
    Assert ($accepted -eq $Accept) ($Name + ': wrong admission result: ' + $failure)
    if ($global:T22AdmissionFixture.noCalls) { Assert ($global:T22AdmissionFixture.calls -eq 0) ($Name + ': provider called before admission') }
    if ($reserved) { Assert ([IO.File]::ReadAllText($reserved) -ceq 'owned collision') 'collision was overwritten' }
    if ($Accept -and $global:T22AdmissionFixture.action -eq 'Query') {
        Assert ($output.Count -eq 0) 'Query leaked output'
        Assert ($global:T22AdmissionFixture.calls -eq 8 -and @($global:T22AdmissionFixture.methodOwners | Sort-Object -Unique).Count -eq 6) 'every NTFS volume needs all three methods'
        $value = [IO.File]::ReadAllText($json) | ConvertFrom-Json
        Assert-Keys $value @('schema','nonce','script_sha256','system_drive','volumes')
        Assert ($value.schema -ceq 'bridgevm.t22-pair-admission.v1' -and $value.nonce -ceq $global:T22AdmissionFixture.nonce -and $value.script_sha256 -ceq $sourceHash -and $value.system_drive -ceq 'C:') 'report identity differs'
        Assert ($value.volumes.Count -eq 4 -and $value.volumes[1].drive_letter -eq $null) 'unlettered volume absent'
        foreach ($volume in $value.volumes) {
            Assert-Keys $volume @('device_id','drive_letter','filesystem','drive_type','conversion_return','conversion_status','encryption_return','encryption_method','protection_return','protection_status')
            foreach ($field in @('conversion_return','conversion_status','encryption_return','encryption_method','protection_return','protection_status')) {
                if ($volume.filesystem -eq 'NTFS') { Assert ($null -ne $volume.$field -and $volume.$field -eq 0) 'NTFS proof missing' }
                else { Assert ($null -eq $volume.$field) 'FAT proof must be null' }
            }
        }
        Assert ([IO.File]::ReadAllText($done) -ceq (Get-FileHash -LiteralPath $json -Algorithm SHA256).Hash.ToLowerInvariant()) 'completion hash differs'
        if ($Name -ceq 'decrypted OS plus unlettered data and FAT inventory') {
            $code = @'
import hashlib, json, os, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
share, nonce, script_hash = Path(sys.argv[2]), sys.argv[3], sys.argv[4]
if os.name == "nt":
    from t22_pair_query import validate_query
    def unique(pairs):
        value = dict(pairs)
        if len(value) != len(pairs): raise ValueError("duplicate fixture JSON field")
        return value
    def reject_constant(value): raise ValueError("nonfinite fixture JSON value")
    raw = (share / ("result-" + nonce + ".json")).read_bytes()
    done = (share / ("result-" + nonce + ".done")).read_bytes()
    assert done == hashlib.sha256(raw).hexdigest().encode("ascii")
    facts = validate_query(json.loads(raw, object_pairs_hook=unique, parse_constant=reject_constant), nonce, script_hash)
    label = "collector field interoperability (Windows fixture reader) PASS"
else:
    from t22_pair_admission import read_query
    facts, digest = read_query(share, nonce, script_hash)
    label = "production Python collector: PASS"
assert facts == {"fixed_volume_count": 4, "decrypted_ntfs_volume_count": 2}
print(label + " fixed=4 ntfs=2")
'@
            $collector = Join-Path $directory 'collector.py'
            [IO.File]::WriteAllText($collector, $code, (New-Object Text.UTF8Encoding($false)))
            $collected = @(& $Python $collector (Join-Path $PSScriptRoot '../../scripts/live-gates') $directory $global:T22AdmissionFixture.nonce $sourceHash)
            $label = if ([IO.Path]::DirectorySeparatorChar -eq '\') { 'collector field interoperability (Windows fixture reader) PASS' } else { 'production Python collector: PASS' }
            Assert ($LASTEXITCODE -eq 0 -and ($collected -join '') -ceq ($label + ' fixed=4 ntfs=2')) 'collector interoperability refused copied-script Query output'
            Write-Output ('COLLECTOR: ' + ($collected -join ''))
        }
    } elseif ($Accept) {
        Assert (($output -join '') -ceq ('T22-PAIR-QUERY-LAUNCHED-' + $global:T22AdmissionFixture.nonce)) 'launch receipt differs'
        Assert ($global:T22AdmissionFixture.launchCommand -ceq "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\t22-pair-proof\bv-t22-pair-admission.ps1 -Action Query -Nonce $($global:T22AdmissionFixture.nonce) -ExpectedSha256 $sourceHash") 'fixed launch command differs'
        Assert (-not (Test-Path -LiteralPath $json) -and -not (Test-Path -LiteralPath $done)) 'Launch queried directly'
    } else {
        Assert (-not (Test-Path -LiteralPath $json) -or $json -ceq $reserved) 'refusal published JSON'
        Assert (-not (Test-Path -LiteralPath $done) -or $done -ceq $reserved) 'refusal published completion'
    }
    Assert ((Get-FileHash -LiteralPath $probe -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $sourceHash) 'production script mutated'
    $global:T22AdmissionFixture.passed++; Write-Output ('PASS: ' + $Name)
}
try {
    Run-Case 'decrypted OS plus unlettered data and FAT inventory' {} $true
    Run-Case 'signed Int32 zero method values' { foreach($reply in $global:T22AdmissionFixture.methods.Values){foreach($key in @($reply.Keys)){$reply[$key]=[int]0}} } $true
    Run-Case 'CIM child launch' { $global:T22AdmissionFixture.action='Launch' } $true
    Run-Case 'signed Int32 launch values' { $global:T22AdmissionFixture.action='Launch'; $global:T22AdmissionFixture.created=[pscustomobject]@{ReturnValue=[int]0; ProcessId=[int]123} } $true
    foreach ($method in @('GetConversionStatus','GetEncryptionMethod','GetProtectionStatus')) {
        $field = @{GetConversionStatus='ConversionStatus'; GetEncryptionMethod='EncryptionMethod'; GetProtectionStatus='ProtectionStatus'}[$method]
        foreach ($property in @('ReturnValue', $field)) {
            Run-Case ($method + '.' + $property + ' nonzero') { $global:T22AdmissionFixture.methods[$method][$property]=[uint32]1 } $false
            foreach ($bad in @($null, $false, '0', [long]0, [double]0, [int]-1)) {
                Run-Case ($method + '.' + $property + ' invalid type ' + $(if($null -eq $bad){'null'}else{$bad.GetType().Name})) { $global:T22AdmissionFixture.methods[$method][$property]=$bad } $false
            }
        }
    }
    Run-Case 'protection zero but partial conversion' { $global:T22AdmissionFixture.methods.GetConversionStatus.ConversionStatus=[uint32]2 } $false
    Run-Case 'unlettered data remains encrypted despite clear OS' { $global:T22AdmissionFixture.overrides[$global:T22AdmissionFixture.ids.data]=@{GetEncryptionMethod=@{ReturnValue=[uint32]0; EncryptionMethod=[uint32]1}} } $false
    Run-Case 'missing unlettered encryption coverage' { $global:T22AdmissionFixture.providers=@($global:T22AdmissionFixture.providers[0]) } $false
    Run-Case 'duplicate provider' { $global:T22AdmissionFixture.providers+= $global:T22AdmissionFixture.providers[0] } $false
    Run-Case 'extra provider absent from inventory' { $global:T22AdmissionFixture.providers+=[pscustomobject]@{DeviceID=$global:T22AdmissionFixture.ids.extra} } $false
    Run-Case 'duplicate inventory' { $global:T22AdmissionFixture.inventory+= $global:T22AdmissionFixture.inventory[0] } $false
    Run-Case 'unknown filesystem' { $global:T22AdmissionFixture.inventory[1].FileSystem='ReFS' } $false
    Run-Case 'null filesystem' { $global:T22AdmissionFixture.inventory[1].FileSystem=$null } $false
    Run-Case 'null DeviceID' { $global:T22AdmissionFixture.inventory[1].DeviceID=$null } $false
    Run-Case 'boolean drive type' { $global:T22AdmissionFixture.inventory[1].DriveType=$true } $false
    Run-Case 'no NTFS SystemDrive' { $env:SystemDrive='Z:' } $false
    Run-Case 'duplicate SystemDrive' { $global:T22AdmissionFixture.inventory[1].DriveLetter='C:' } $false
    Run-Case 'provider failure' { $global:T22AdmissionFixture.providerError=$true } $false
    Run-Case 'method failure' { $global:T22AdmissionFixture.methodError=$true } $false
    Run-Case 'wrong script digest' { $global:T22AdmissionFixture.expected='0'*64; $global:T22AdmissionFixture.noCalls=$true } $false
    Run-Case 'invalid nonce' { $global:T22AdmissionFixture.nonce='A'*32; $global:T22AdmissionFixture.noCalls=$true } $false
    Run-Case 'nonce trailing newline' { $global:T22AdmissionFixture.nonce=('a'*32)+"`n"; $global:T22AdmissionFixture.noCalls=$true } $false
    foreach ($suffix in @('.json','.done','.pending')) {
        Run-Case ('exclusive name collision ' + $suffix) { $global:T22AdmissionFixture.collision=$suffix; $global:T22AdmissionFixture.noCalls=$true } $false
    }
    foreach ($property in @('ReturnValue','ProcessId')) {
        Run-Case ('launch rejects boolean ' + $property) { $global:T22AdmissionFixture.action='Launch'; $global:T22AdmissionFixture.created.$property=$false } $false
        Run-Case ('launch rejects null ' + $property) { $global:T22AdmissionFixture.action='Launch'; $global:T22AdmissionFixture.created.$property=$null } $false
    }
    Run-Case 'launch refuses process zero' { $global:T22AdmissionFixture.action='Launch'; $global:T22AdmissionFixture.created.ProcessId=[uint32]0 } $false
    Run-Case 'launch refuses create error' { $global:T22AdmissionFixture.action='Launch'; $global:T22AdmissionFixture.created.ReturnValue=[uint32]1 } $false
    Assert ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $sourceHash) 'source changed during execution'
    Write-Output ('PowerShell version: ' + $PSVersionTable.PSVersion.ToString())
    Write-Output ("T22 pair admission PowerShell: $($global:T22AdmissionFixture.passed) cases PASS; mocked CIM only")
} finally {
    $env:SystemDrive = $previousDrive
    Remove-Item -LiteralPath $root -Recurse -Force
    Remove-Variable -Scope Global -Name T22AdmissionFixture
}
