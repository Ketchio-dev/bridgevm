trap { "BVERR " + ($_ | Out-String); continue }
$ErrorActionPreference = "Stop"
# Report every display: device, adapter, current mode and how many modes it lists.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct DISPLAY_DEVICE {
  public int cb;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
  public int StateFlags;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
}
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct DEVMODE2 {
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
public class BvRep {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern bool EnumDisplayDevicesW(string dev, uint i, ref DISPLAY_DEVICE dd, uint flags);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern bool EnumDisplaySettingsW(string dev, int mode, ref DEVMODE2 dm);
}
'@
$i = 0
while ($true) {
  $dd = New-Object DISPLAY_DEVICE; $dd.cb = [Runtime.InteropServices.Marshal]::SizeOf([type][DISPLAY_DEVICE])
  if (-not [BvRep]::EnumDisplayDevicesW([NullString]::Value, $i, [ref]$dd, 0)) { break }
  $i++
  $dm = New-Object DEVMODE2; $dm.dmSize = [Runtime.InteropServices.Marshal]::SizeOf([type][DEVMODE2])
  $cur = if ([BvRep]::EnumDisplaySettingsW($dd.DeviceName, -1, [ref]$dm)) { "{0}x{1}" -f $dm.dmPelsWidth, $dm.dmPelsHeight } else { "none" }
  $n = 0; $max = ""
  while ($true) {
    $m = New-Object DEVMODE2; $m.dmSize = $dm.dmSize
    if (-not [BvRep]::EnumDisplaySettingsW($dd.DeviceName, $n, [ref]$m)) { break }
    $max = "{0}x{1}" -f $m.dmPelsWidth, $m.dmPelsHeight; $n++
  }
  "BVDISP name={0} adapter=""{1}"" active={2} primary={6} pos={7},{8} current={3} modes={4} last={5}" -f $dd.DeviceName, $dd.DeviceString, (($dd.StateFlags -band 1) -ne 0), $cur, $n, $max, (($dd.StateFlags -band 4) -ne 0), $dm.dmPositionX, $dm.dmPositionY
}
