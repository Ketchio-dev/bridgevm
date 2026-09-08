param([Parameter(Mandatory=$true)][long]$Hwnd)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class BvGlyphDpi {
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowDpiAwarenessContext(IntPtr window);
  [DllImport("user32.dll")] public static extern int GetAwarenessFromDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
  [DllImport("shcore.dll")] public static extern int GetScaleFactorForMonitor(IntPtr monitor, out int scale);
}
'@
$window = [IntPtr]$Hwnd
if (-not [BvGlyphDpi]::IsWindow($window)) { throw 'Window does not exist' }
$dpi = [BvGlyphDpi]::GetDpiForWindow($window)
$awareness = [BvGlyphDpi]::GetAwarenessFromDpiAwarenessContext([BvGlyphDpi]::GetWindowDpiAwarenessContext($window))
$monitor = [BvGlyphDpi]::MonitorFromWindow($window, 0)
$scale = 0
$result = [BvGlyphDpi]::GetScaleFactorForMonitor($monitor, [ref]$scale)
if ($dpi -eq 0 -or $monitor -eq [IntPtr]::Zero -or $result -ne 0) { throw 'Effective DPI query failed' }
Write-Output "BVEFFECTIVEDPI hwnd=$Hwnd dpi=$dpi awareness=$awareness monitor_scale=$scale"
