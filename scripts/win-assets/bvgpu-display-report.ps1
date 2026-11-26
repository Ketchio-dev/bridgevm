param([uint32]$RefreshHz = 0)
trap {
  "BVERR " + ($_ | Out-String)
  if ($RefreshHz) { exit 1 }
  continue
}
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
  [DllImport("user32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
  static extern int ChangeDisplaySettingsExW(string dev, ref DEVMODE2 dm, IntPtr hwnd, uint flags, IntPtr data);

  static bool SameScanout(DEVMODE2 a, DEVMODE2 b) {
    return a.dmPelsWidth == b.dmPelsWidth && a.dmPelsHeight == b.dmPelsHeight
        && a.dmBitsPerPel == b.dmBitsPerPel && a.dmDisplayOrientation == b.dmDisplayOrientation
        && a.dmDisplayFlags == b.dmDisplayFlags;
  }

  public static string SelectRefresh(string dev, DEVMODE2 current, uint hz) {
    const uint DM_DISPLAYFREQUENCY = 0x00400000;
    const uint CDS_TEST = 2, CDS_UPDATEREGISTRY = 1;
    const int DISP_CHANGE_SUCCESSFUL = 0, DISP_CHANGE_RESTART = 1;
    for (int index = 0; ; ++index) {
      var mode = new DEVMODE2 { dmSize = current.dmSize };
      if (!EnumDisplaySettingsW(dev, index, ref mode))
        throw new InvalidOperationException("The current resolution does not offer " + hz + " Hz.");
      if (mode.dmDisplayFrequency != hz || !SameScanout(mode, current)) continue;

      current.dmFields = DM_DISPLAYFREQUENCY;
      current.dmDisplayFrequency = hz;
      int test = ChangeDisplaySettingsExW(dev, ref current, IntPtr.Zero, CDS_TEST, IntPtr.Zero);
      if (test != DISP_CHANGE_SUCCESSFUL)
        throw new InvalidOperationException("Windows rejected the requested rate (test " + test + ").");
      int save = ChangeDisplaySettingsExW(dev, ref current, IntPtr.Zero, CDS_UPDATEREGISTRY, IntPtr.Zero);
      if (save != DISP_CHANGE_SUCCESSFUL && save != DISP_CHANGE_RESTART)
        throw new InvalidOperationException("Windows could not save the requested rate (" + save + ").");

      var saved = new DEVMODE2 { dmSize = current.dmSize };
      if (!EnumDisplaySettingsW(dev, -2, ref saved) || !SameScanout(saved, mode) || saved.dmDisplayFrequency != hz)
        throw new InvalidOperationException("The saved display mode did not match the request.");
      if (save == DISP_CHANGE_RESTART)
        return "BVDISP refresh name=" + dev + " saved_hz=" + hz + " restart_required=yes";

      var active = new DEVMODE2 { dmSize = current.dmSize };
      if (!EnumDisplaySettingsW(dev, -1, ref active) || !SameScanout(active, mode) || active.dmDisplayFrequency != hz)
        throw new InvalidOperationException("The active display mode did not match the request.");
      return "BVDISP refresh name=" + dev + " active_hz=" + hz + " saved_hz=" + hz;
    }
  }
}
'@
$refreshDisplay = $null
$i = 0
while ($true) {
  $dd = New-Object DISPLAY_DEVICE; $dd.cb = [Runtime.InteropServices.Marshal]::SizeOf([type][DISPLAY_DEVICE])
  if (-not [BvRep]::EnumDisplayDevicesW([NullString]::Value, $i, [ref]$dd, 0)) { break }
  $i++
  $dm = New-Object DEVMODE2; $dm.dmSize = [Runtime.InteropServices.Marshal]::SizeOf([type][DEVMODE2])
  $hasCurrent = [BvRep]::EnumDisplaySettingsW($dd.DeviceName, -1, [ref]$dm)
  $cur = if ($hasCurrent) { "{0}x{1}" -f $dm.dmPelsWidth, $dm.dmPelsHeight } else { "none" }
  if ($RefreshHz -and $hasCurrent -and (($dd.StateFlags -band 1) -ne 0) -and
      ($dd.DeviceID -like 'PCI\VEN_1AF4&DEV_1050*')) {
    if ($refreshDisplay) { throw "More than one virtio-gpu display is attached." }
    $refreshDisplay = $dd.DeviceName
    $refreshMode = $dm
  }
  $n = 0; $max = ""
  while ($true) {
    $m = New-Object DEVMODE2; $m.dmSize = $dm.dmSize
    if (-not [BvRep]::EnumDisplaySettingsW($dd.DeviceName, $n, [ref]$m)) { break }
    $max = "{0}x{1}" -f $m.dmPelsWidth, $m.dmPelsHeight; $n++
  }
  "BVDISP name={0} adapter=""{1}"" active={2} primary={6} pos={7},{8} current={3} modes={4} last={5}" -f $dd.DeviceName, $dd.DeviceString, (($dd.StateFlags -band 1) -ne 0), $cur, $n, $max, (($dd.StateFlags -band 4) -ne 0), $dm.dmPositionX, $dm.dmPositionY
}

if ($RefreshHz) {
  if (-not $refreshDisplay) { throw "No active virtio-gpu display is available." }
  [BvRep]::SelectRefresh($refreshDisplay, $refreshMode, $RefreshHz)
}
