param(
    [Parameter(Mandatory = $true)][string]$Nonce,
    [Parameter(Mandatory = $true)][string]$ConfigPath,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

if ($Nonce -notmatch '^[0-9a-f]{32}$') { throw 'invalid nonce' }
$ConfigBytes = [System.IO.File]::ReadAllBytes($ConfigPath)
$ConfigSha256 = ([System.BitConverter]::ToString(
    [System.Security.Cryptography.SHA256]::Create().ComputeHash($ConfigBytes)
)).Replace('-', '').ToLowerInvariant()
$Config = [System.Text.Encoding]::UTF8.GetString($ConfigBytes) | ConvertFrom-Json
if ($Config.schema -ne 'bridgevm.hvf-nvme-quiescence.v1' -or
    [int]$Config.samples -ne 30 -or [int]$Config.interval_seconds -ne 1 -or
    [double]$Config.cpu_percent.median_max -ne 10 -or
    [double]$Config.cpu_percent.p95_max -ne 20 -or
    [double]$Config.disk_bytes_per_second.median_max -ne 1048576 -or
    [double]$Config.disk_bytes_per_second.p95_max -ne 4194304 -or
    [double]$Config.disk_queue_length.p95_max -ne 0.25 -or
    [int]$Config.post_ready_settle_seconds -ne 120 -or
    [int]$Config.post_sample_quiet_seconds -ne 15) {
    throw 'quiescence config is not the fixed v1 policy'
}

$Defender = Get-MpComputerStatus
if ($null -eq $Defender -or -not [bool]$Defender.AntivirusEnabled -or
    -not [bool]$Defender.RealTimeProtectionEnabled) {
    throw 'Windows security services are not enabled'
}
$Samples = @()
for ($Ordinal = 1; $Ordinal -le 30; $Ordinal++) {
    $Processor = Get-CimInstance -ClassName Win32_PerfFormattedData_PerfOS_Processor |
        Where-Object { $_.Name -eq '_Total' } | Select-Object -First 1
    $Disk = Get-CimInstance -ClassName Win32_PerfFormattedData_PerfDisk_PhysicalDisk |
        Where-Object { $_.Name -eq '_Total' } | Select-Object -First 1
    if ($null -eq $Processor -or $null -eq $Disk) { throw 'missing performance counter' }
    $Cpu = [double]$Processor.PercentProcessorTime
    $BytesPerSecond = [double]$Disk.DiskBytesPersec
    $QueueLength = [double]$Disk.AvgDiskQueueLength
    if ([double]::IsNaN($Cpu) -or [double]::IsInfinity($Cpu) -or $Cpu -lt 0 -or
        [double]::IsNaN($BytesPerSecond) -or [double]::IsInfinity($BytesPerSecond) -or
        $BytesPerSecond -lt 0 -or [double]::IsNaN($QueueLength) -or
        [double]::IsInfinity($QueueLength) -or $QueueLength -lt 0) {
        throw 'invalid performance counter value'
    }
    $Samples += [ordered]@{
        ordinal = $Ordinal
        cpu_percent = $Cpu
        disk_bytes_per_second = $BytesPerSecond
        disk_queue_length = $QueueLength
    }
    if ($Ordinal -lt 30) { Start-Sleep -Seconds 1 }
}

$Document = [ordered]@{
    schema = 'bridgevm.hvf-nvme-quiescence-result.v1'
    nonce = $Nonce
    config_sha256 = $ConfigSha256
    security_services_enabled = $true
    samples = $Samples
}
$Temporary = "$OutputPath.tmp.$PID"
[System.IO.File]::WriteAllText(
    $Temporary,
    ($Document | ConvertTo-Json -Depth 4 -Compress) + "`n",
    $Utf8NoBom
)
Move-Item -LiteralPath $Temporary -Destination $OutputPath -ErrorAction Stop
