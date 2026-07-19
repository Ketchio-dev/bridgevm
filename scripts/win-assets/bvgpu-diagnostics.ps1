[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$gpuPattern = '^PCI\\VEN_1AF4&DEV_(1050|10F7)(?:&|$)'
$boot = (Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).LastBootUpTime
if (-not $boot) { $boot = (Get-Date).AddHours(-1) }

Write-Output ('[diagnostics] captured_utc=' + [DateTime]::UtcNow.ToString('o'))
Write-Output ('[diagnostics] boot_utc=' + $boot.ToUniversalTime().ToString('o'))

$enumRoot = 'HKLM:\SYSTEM\CurrentControlSet\Enum\PCI'
$instanceIds = @()
if (Test-Path -LiteralPath $enumRoot) {
  $hardwareKeys = @(Get-ChildItem -LiteralPath $enumRoot -ErrorAction SilentlyContinue |
    Where-Object { $_.PSChildName -match '^VEN_1AF4&DEV_(1050|10F7)(?:&|$)' })
  foreach ($hardwareKey in $hardwareKeys) {
    $hardwareId = $hardwareKey.PSChildName
    foreach ($instanceKey in @(Get-ChildItem -LiteralPath $hardwareKey.PSPath -ErrorAction SilentlyContinue)) {
      $instanceId = 'PCI\' + $hardwareId + '\' + $instanceKey.PSChildName
      $raw = Get-ItemProperty -LiteralPath $instanceKey.PSPath -ErrorAction SilentlyContinue
      Write-Output ('[diagnostics] enum instance=' + $instanceId +
        ' service=' + $raw.Service + ' driver=' + $raw.Driver +
        ' config_flags=' + $raw.ConfigFlags + ' problem=' + $raw.Problem +
        ' problem_status=' + $raw.ProblemStatus)
      $instanceIds += $instanceId
    }
  }
}

# Query exact registry-derived instance IDs. Enumerating every PnP device was
# slow enough to lose the decisive Code 43 evidence when the VM watchdog fired.
$devices = @($instanceIds | ForEach-Object {
  Get-PnpDevice -InstanceId $_ -ErrorAction SilentlyContinue
} | Where-Object { $_ -and $_.InstanceId -match $gpuPattern })
if ($devices.Count -eq 0) {
  Write-Output '[diagnostics] no VirtIO GPU PnP device object found'
}

foreach ($dev in $devices) {
  $problem = Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction SilentlyContinue
  $problemStatus = Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_ProblemStatus' -ErrorAction SilentlyContinue
  $devNodeStatus = Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_DevNodeStatus' -ErrorAction SilentlyContinue
  $isPresent = Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_IsPresent' -ErrorAction SilentlyContinue
  $driverKey = Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_Driver' -ErrorAction SilentlyContinue
  $signed = Get-CimInstance Win32_PnPSignedDriver -ErrorAction SilentlyContinue |
    Where-Object { $_.DeviceID -eq $dev.InstanceId } | Select-Object -First 1

  Write-Output ('[diagnostics] pnp instance=' + $dev.InstanceId)
  Write-Output ('[diagnostics] pnp status=' + $dev.Status + ' class=' + $dev.Class +
    ' friendly_name=' + $dev.FriendlyName + ' problem_code=' + $problem.Data +
    ' problem_status=' + $problemStatus.Data + ' devnode_status=' + $devNodeStatus.Data +
    ' is_present=' + $isPresent.Data)
  if ($signed) {
    Write-Output ('[diagnostics] driver inf=' + $signed.InfName + ' version=' +
      $signed.DriverVersion + ' provider=' + $signed.DriverProviderName +
      ' device_name=' + $signed.DeviceName)
  }

  if ($driverKey.Data) {
    $classPath = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\' + $driverKey.Data
    Write-Output ('[diagnostics] class_key=' + $classPath)
    if (Test-Path -LiteralPath $classPath) {
      $classValues = Get-ItemProperty -LiteralPath $classPath -ErrorAction SilentlyContinue
      foreach ($name in @(
        'InstalledDisplayDrivers',
        'UserModeDriverName',
        'UserModeDriverNameWow',
        'VulkanDriverName',
        'OpenGLDriverName',
        'OpenGLFlags',
        'OpenGLVersion'
      )) {
        $value = $classValues.$name
        if ($null -ne $value) {
          Write-Output ('[diagnostics] class_value ' + $name + '=' + (($value | ForEach-Object { [string]$_ }) -join ';'))
        } else {
          Write-Output ('[diagnostics] class_value ' + $name + '=<absent>')
        }
      }
    } else {
      Write-Output '[diagnostics] class key is absent'
    }
  }
}

foreach ($logName in @('System', 'Microsoft-Windows-DxgKrnl/Operational')) {
  try {
    $events = @(Get-WinEvent -FilterHashtable @{
        LogName = $logName
        StartTime = $boot.AddMinutes(-2)
      } -MaxEvents 300 -ErrorAction Stop | Where-Object {
        $_.ProviderName -match 'Kernel-PnP|UserPnp|DxgKrnl' -or
        ([string]$_.Message) -match 'VEN_1AF4|viogpu3d|VioGpu3D'
      } | Select-Object -First 100)
  } catch {
    Write-Output ('[diagnostics] event_query_failed log=' + $logName + ' error=' + $_.Exception.Message)
    $events = @()
  }
  foreach ($event in $events) {
    $message = ([string]$event.Message) -replace '[\r\n]+', ' '
    Write-Output ('[diagnostics] event log=' + $logName + ' provider=' +
      $event.ProviderName + ' id=' + $event.Id + ' level=' + $event.LevelDisplayName +
      ' utc=' + $event.TimeCreated.ToUniversalTime().ToString('o') + ' message=' + $message)
  }
}

$setupApi = Join-Path $env:windir 'INF\setupapi.dev.log'
if (Test-Path -LiteralPath $setupApi -PathType Leaf) {
  Write-Output '[diagnostics] recent SetupAPI lines mentioning the VirtIO GPU or viogpu3d:'
  Get-Content -LiteralPath $setupApi -Tail 2000 -ErrorAction SilentlyContinue |
    Select-String -Pattern 'VEN_1AF4&DEV_(1050|10F7)|viogpu3d' -CaseSensitive:$false |
    Select-Object -Last 80 | ForEach-Object { Write-Output ('[diagnostics] setupapi ' + $_.Line) }
}

Write-Output '[diagnostics] complete'
