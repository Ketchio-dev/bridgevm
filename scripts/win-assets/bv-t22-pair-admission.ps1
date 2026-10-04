[CmdletBinding()]
param([string]$Action, [string]$Nonce, [string]$ExpectedSha256)
# Read-only Windows volume admission; neither TPM independence nor readiness proof.
$ErrorActionPreference = 'Stop'
if ($ExpectedSha256 -cnotmatch '^[0-9a-f]{64}\z') { throw 'invalid script digest' }
$ActualSha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($ActualSha256 -cne $ExpectedSha256) { throw 'script digest mismatch' }
if ($Nonce -cnotmatch '^[0-9a-f]{32}\z') { throw 'invalid nonce' }
if ($Action -cne 'Launch' -and $Action -cne 'Query') { throw 'invalid action' }

function Require-UInt32($Value, [string]$Field) {
    if (($Value -isnot [uint32] -and $Value -isnot [int]) -or $Value -lt 0) {
        throw ('invalid integer: ' + $Field)
    }
    return [uint32]$Value
}
function Require-DeviceId($Value) {
    if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value) -or
        $Value.Length -gt 4096 -or $Value.Contains("`r") -or $Value.Contains("`n")) {
        throw 'invalid volume DeviceID'
    }
    return $Value
}
function Require-ZeroMethod($Volume, [string]$Method, [string]$Field, [hashtable]$Arguments) {
    $reply = Invoke-CimMethod -InputObject $Volume -MethodName $Method -Arguments $Arguments -ErrorAction Stop
    $returned = Require-UInt32 $reply.ReturnValue ($Method + '.ReturnValue')
    $value = Require-UInt32 $reply.$Field ($Method + '.' + $Field)
    if ($returned -ne 0 -or $value -ne 0) { throw ('volume is not fully decrypted and unprotected: ' + $Method) }
    return @{ Returned = $returned; Value = $value }
}
if ($Action -eq 'Launch') {
    $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\t22-pair-proof\bv-t22-pair-admission.ps1 -Action Query -Nonce $Nonce -ExpectedSha256 $ExpectedSha256"
    $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $command } -ErrorAction Stop
    $returned = Require-UInt32 $created.ReturnValue 'Create.ReturnValue'
    $pidValue = Require-UInt32 $created.ProcessId 'Create.ProcessId'
    if ($returned -ne 0 -or $pidValue -eq 0) { throw 'query workload did not launch' }
    Write-Output "T22-PAIR-QUERY-LAUNCHED-$Nonce"
    return
}

$result = Join-Path $PSScriptRoot "result-$Nonce.json"
$done = Join-Path $PSScriptRoot "result-$Nonce.done"
$pending = Join-Path $PSScriptRoot "result-$Nonce.pending"
foreach ($name in @($result, $done, $pending)) {
    if ($null -ne (Get-Item -LiteralPath $name -Force -ErrorAction SilentlyContinue)) { throw 'result name already exists' }
}
# CreateNew reserves this query before any provider call. Refusals retain it.
$stream = [IO.File]::Open($pending, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $systemDrive = $env:SystemDrive
    if ($systemDrive -cnotmatch '^[A-Z]:\z') { throw 'invalid SystemDrive' }
    $inventory = @(Get-CimInstance -ClassName Win32_Volume -Filter 'DriveType=3' -ErrorAction Stop)
    $providers = @(Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume -ErrorAction Stop)
    $volumes = @{}
    $letters = @{}
    $ntfs = @{}
    $records = @()
    $systemCount = 0
    foreach ($volume in $inventory) {
        $id = Require-DeviceId $volume.DeviceID
        if ($volumes.ContainsKey($id)) { throw 'duplicate inventory DeviceID' }
        $type = Require-UInt32 $volume.DriveType 'DriveType'
        if ($type -ne 3) { throw 'inventory contains a non-fixed volume' }
        $filesystem = $volume.FileSystem
        if ($filesystem -isnot [string] -or $filesystem.ToUpperInvariant() -notin @('NTFS', 'FAT', 'FAT32')) {
            throw 'unknown fixed-volume filesystem'
        }
        $filesystem = $filesystem.ToUpperInvariant()
        $letter = $volume.DriveLetter
        if ($null -ne $letter) {
            if ($letter -isnot [string] -or $letter -cnotmatch '^[A-Z]:\z' -or $letters.ContainsKey($letter)) {
                throw 'invalid or duplicate drive letter'
            }
            $letters[$letter] = $true
        }
        $volumes[$id] = $volume
        if ($filesystem -eq 'NTFS') {
            $ntfs[$id] = $volume
            if ($letter -ceq $systemDrive) { $systemCount++ }
        }
        $records += [ordered]@{
            device_id = $id; drive_letter = $letter; filesystem = $filesystem; drive_type = $type
            conversion_return = $null; conversion_status = $null
            encryption_return = $null; encryption_method = $null
            protection_return = $null; protection_status = $null
        }
    }
    if ($systemCount -ne 1) { throw 'SystemDrive must identify exactly one NTFS volume' }
    $covered = @{}
    foreach ($provider in $providers) {
        $id = Require-DeviceId $provider.DeviceID
        if ($covered.ContainsKey($id) -or -not $ntfs.ContainsKey($id)) {
            throw 'duplicate or unlisted encryption-provider volume'
        }
        $covered[$id] = $provider
    }
    if ($covered.Count -ne $ntfs.Count) { throw 'incomplete encryption-provider coverage' }
    foreach ($record in $records) {
        if ($record.filesystem -ne 'NTFS') { continue }
        $provider = $covered[$record.device_id]
        $conversion = Require-ZeroMethod $provider GetConversionStatus ConversionStatus @{ PrecisionFactor = [uint32]0 }
        $encryption = Require-ZeroMethod $provider GetEncryptionMethod EncryptionMethod @{}
        $protection = Require-ZeroMethod $provider GetProtectionStatus ProtectionStatus @{}
        $record.conversion_return = $conversion.Returned; $record.conversion_status = $conversion.Value
        $record.encryption_return = $encryption.Returned; $record.encryption_method = $encryption.Value
        $record.protection_return = $protection.Returned; $record.protection_status = $protection.Value
    }
    $report = [ordered]@{
        schema = 'bridgevm.t22-pair-admission.v1'; nonce = $Nonce; script_sha256 = $ActualSha256
        system_drive = $systemDrive; volumes = @($records)
    }
    $bytes = (New-Object Text.UTF8Encoding($false)).GetBytes(($report | ConvertTo-Json -Depth 5 -Compress))
    $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true)
} finally { $stream.Dispose() }
[IO.File]::Move($pending, $result)
$sha = [Security.Cryptography.SHA256]::Create()
try { $digest = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '').ToLowerInvariant() }
finally { $sha.Dispose() }
$completion = [IO.File]::Open($done, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $bytes = [Text.Encoding]::ASCII.GetBytes($digest)
    $completion.Write($bytes, 0, $bytes.Length); $completion.Flush($true)
} finally { $completion.Dispose() }
