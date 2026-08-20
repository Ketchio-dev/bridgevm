param(
    [ValidateSet('F1', 'Display', 'Window', 'Notepad')]
    [string]$Action = 'F1',
    [long]$Hwnd = 0
)
$ErrorActionPreference = 'Stop'

function Get-ProblemCode($Device) {
    try {
        $property = Get-PnpDeviceProperty -InstanceId $Device.InstanceId `
            -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction Stop
        return [uint32]$property.Data
    } catch {
        return [uint32]0xffffffff
    }
}

function Add-DisplayApi {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct BvClosureDevMode {
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
  public ushort dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra;
  public uint dmFields;
  public int dmPositionX, dmPositionY;
  public uint dmDisplayOrientation, dmDisplayFixedOutput;
  public short dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
  public ushort dmLogPixels;
  public uint dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency;
  public uint dmICMMethod, dmICMIntent, dmMediaType, dmDitherType;
  public uint dmReserved1, dmReserved2, dmPanningWidth, dmPanningHeight;
}
public static class BvClosureDisplay {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern bool EnumDisplaySettingsW(string device, int mode, ref BvClosureDevMode value);
}
'@
}

function Get-DisplayProof {
    Add-DisplayApi
    $size = [uint16][Runtime.InteropServices.Marshal]::SizeOf([type][BvClosureDevMode])
    $best = $null
    foreach ($number in 1..8) {
        $device = "\\.\DISPLAY$number"
        $current = New-Object BvClosureDevMode
        $current.dmSize = $size
        if (-not [BvClosureDisplay]::EnumDisplaySettingsW($device, -1, [ref]$current)) { continue }
        $modes = New-Object System.Collections.Generic.List[string]
        $index = 0
        while ($true) {
            $mode = New-Object BvClosureDevMode
            $mode.dmSize = $size
            if (-not [BvClosureDisplay]::EnumDisplaySettingsW($device, $index, [ref]$mode)) { break }
            $name = "$($mode.dmPelsWidth)x$($mode.dmPelsHeight)"
            if (-not $modes.Contains($name)) { $modes.Add($name) }
            $index++
        }
        $record = [pscustomobject]@{
            Device = $device
            Current = "$($current.dmPelsWidth)x$($current.dmPelsHeight)"
            Count = $index
            Modes = $modes
        }
        if ($null -eq $best -or $record.Count -gt $best.Count) { $best = $record }
    }
    if ($null -eq $best) { throw 'no Windows display enumerated' }
    return $best
}

function Write-F1Proof {
    $bcd = (& bcdedit.exe /enum '{current}' 2>&1 | Out-String)
    $testsigning = [bool]($bcd -match '(?im)^testsigning\s+Yes\s*$')
    $displays = @(Get-PnpDevice -Class Display -ErrorAction Stop)
    $viogpu = $displays | Where-Object { $_.InstanceId -match '^PCI\\VEN_1AF4&DEV_10F7' } | Select-Object -First 1
    $vioserial = Get-PnpDevice -ErrorAction Stop |
        Where-Object { $_.InstanceId -match '^PCI\\VEN_1AF4&DEV_1043' } | Select-Object -First 1
    if ($null -eq $viogpu -or $null -eq $vioserial) { throw 'required virtio PnP device absent' }
    $viogpuProblem = Get-ProblemCode $viogpu
    $vioserialProblem = Get-ProblemCode $vioserial
    $agentHash = (Get-FileHash -Algorithm SHA256 -LiteralPath C:\bvagent.ps1).Hash.ToLowerInvariant()
    $display = Get-DisplayProof
    $hasRequested = $display.Modes.Contains('1600x900')
    Write-Output ("BVF1 testsigning=$testsigning viogpu_status=$($viogpu.Status) " +
        "viogpu_problem=$viogpuProblem vioserial_status=$($vioserial.Status) " +
        "vioserial_problem=$vioserialProblem agent_sha256=$agentHash")
    Write-Output ("BVF1MODE device=$($display.Device) current=$($display.Current) " +
        "modes=$($display.Count) has_1600x900=$hasRequested list=$($display.Modes -join ',')")
    if (-not $testsigning -or $viogpu.Status -ne 'OK' -or $viogpuProblem -ne 0 -or
        $vioserial.Status -ne 'OK' -or $vioserialProblem -ne 0 -or
        $display.Count -le 1 -or -not $hasRequested) { exit 10 }
}

function Write-DisplayProof {
    $display = Get-DisplayProof
    Write-Output ("BVF2 device=$($display.Device) current=$($display.Current) " +
        "modes=$($display.Count) has_1600x900=$($display.Modes.Contains('1600x900'))")
    if ($display.Count -le 1) { exit 11 }
}

function Add-WindowApi {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)]
public struct BvClosureRect { public int Left, Top, Right, Bottom; }
public static class BvClosureWindow {
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out BvClosureRect rect);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
}
'@
}

function Write-WindowProof {
    if ($Hwnd -le 0) { throw 'Window action requires -Hwnd' }
    Add-WindowApi
    $window = [IntPtr]$Hwnd
    $exists = [BvClosureWindow]::IsWindow($window)
    $rect = New-Object BvClosureRect
    $rectOk = $exists -and [BvClosureWindow]::GetWindowRect($window, [ref]$rect)
    # $owner: assigning the read-only automatic $pid throws under Stop preference.
    $owner = [uint32]0
    if ($exists) { [void][BvClosureWindow]::GetWindowThreadProcessId($window, [ref]$owner) }
    $foreground = [BvClosureWindow]::GetForegroundWindow().ToInt64()
    $geometry = if ($rectOk) {
        "$($rect.Left),$($rect.Top),$($rect.Right - $rect.Left),$($rect.Bottom - $rect.Top)"
    } else { 'unavailable' }
    Write-Output "BVWINDOW hwnd=$Hwnd exists=$exists pid=$owner rect=$geometry foreground=$foreground"
    if (-not $exists -or -not $rectOk) { exit 12 }
}

function Write-NotepadProof {
    $processes = @(Get-Process -Name notepad -ErrorAction SilentlyContinue)
    $ids = @($processes | ForEach-Object { $_.Id }) -join ','
    Write-Output "BVNOTEPAD count=$($processes.Count) pids=$ids"
}

switch ($Action) {
    'F1' { Write-F1Proof }
    'Display' { Write-DisplayProof }
    'Window' { Write-WindowProof }
    'Notepad' { Write-NotepadProof }
}
