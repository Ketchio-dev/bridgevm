# Apply the geometry the host published to the virtio-gpu display.
#
# The viogpu3d miniport re-reads GET_DISPLAY_INFO on a resize event and installs
# the new geometry in its mode table, but it never asks the OS to re-enumerate,
# and upstream's viogpuap.exe -- which would do the mode switch -- is not part of
# the 3D driver package. So the mode is available and simply never selected.
#
# This selects it. It also picks the right display: \\.\DISPLAY1 is the Microsoft
# Basic Display Driver on this image and reports a single 800x600 mode, while the
# virtio-gpu adapter is a later DISPLAY number.

param([int]$Width = 0, [int]$Height = 0, [int]$TimeoutSeconds = 30)

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct DEVMODE {
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
public class BvDisp {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern bool EnumDisplaySettingsW(string dev, int mode, ref DEVMODE dm);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int ChangeDisplaySettingsExW(string dev, ref DEVMODE dm, IntPtr wnd, uint flags, IntPtr param);
}
'@

$size = [uint16][System.Runtime.InteropServices.Marshal]::SizeOf([type][DEVMODE])

function Get-Current([string]$dev) {
  $dm = New-Object DEVMODE
  $dm.dmSize = $size
  if ([BvDisp]::EnumDisplaySettingsW($dev, -1, [ref]$dm)) { return $dm }
  return $null
}

# The virtio-gpu display is the one with a real mode list; the basic display
# driver exposes exactly one.
$target = $null
$targetModes = 0
foreach ($n in 1..8) {
  $dev = "\\.\DISPLAY$n"
  $cur = Get-Current $dev
  if (-not $cur) { continue }
  $count = 0
  $probe = New-Object DEVMODE
  $probe.dmSize = $size
  while ([BvDisp]::EnumDisplaySettingsW($dev, $count, [ref]$probe)) { $count++ }
  Write-Output ("BV-APPLY| $dev current=" + $cur.dmPelsWidth + "x" + $cur.dmPelsHeight + " modes=$count")
  if ($count -gt $targetModes) { $targetModes = $count; $target = $dev }
}
if (-not $target) {
  Write-Output 'BV-APPLY| no_display_found'
  exit 2
}
Write-Output "BV-APPLY| target=$target"

# Without an explicit geometry, take whatever the host most recently published,
# which the driver reports through the monitor's supported source modes.
if ($Width -le 0 -or $Height -le 0) {
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  do {
    $modes = Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorListedSupportedSourceModes -ErrorAction SilentlyContinue
    if ($modes) {
      $pref = $modes[0].MonitorSourceModes | Select-Object -Last 1
      if ($pref) { $Width = $pref.HorizontalActivePixels; $Height = $pref.VerticalActivePixels }
    }
    if ($Width -gt 0 -and $Height -gt 0) { break }
    Start-Sleep -Milliseconds 500
  } while ((Get-Date) -lt $deadline)
}
if ($Width -le 0 -or $Height -le 0) {
  Write-Output 'BV-APPLY| no_target_geometry'
  exit 3
}

$before = Get-Current $target
Write-Output ("BV-APPLY| before=" + $before.dmPelsWidth + "x" + $before.dmPelsHeight + " requested=${Width}x${Height}")
if ($before.dmPelsWidth -eq $Width -and $before.dmPelsHeight -eq $Height) {
  Write-Output ("BV-APPLY| after=${Width}x${Height}")
  Write-Output 'BV-APPLY-DONE'
  exit 0
}

$dm = $before
$dm.dmPelsWidth = [uint32]$Width
$dm.dmPelsHeight = [uint32]$Height
$dm.dmFields = 0x80000 -bor 0x100000   # DM_PELSWIDTH | DM_PELSHEIGHT
$rc = [BvDisp]::ChangeDisplaySettingsExW($target, [ref]$dm, [IntPtr]::Zero, 0x1, [IntPtr]::Zero)
Write-Output "BV-APPLY| change_rc=$rc"

$deadline = (Get-Date).AddSeconds(10)
do {
  Start-Sleep -Milliseconds 500
  $after = Get-Current $target
} while ((Get-Date) -lt $deadline -and ($after.dmPelsWidth -ne $Width -or $after.dmPelsHeight -ne $Height))

Write-Output ("BV-APPLY| after=" + $after.dmPelsWidth + "x" + $after.dmPelsHeight)
Write-Output 'BV-APPLY-DONE'
if ($after.dmPelsWidth -eq $Width -and $after.dmPelsHeight -eq $Height) { exit 0 } else { exit 1 }
