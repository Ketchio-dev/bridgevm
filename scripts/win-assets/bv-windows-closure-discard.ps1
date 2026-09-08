param([Parameter(Mandatory = $true)][long]$Hwnd)
$ErrorActionPreference = 'Stop'
# WM_CLOSE on a modified document raises the save prompt and the window stays
# (t7-7f31bfc8-keeprunning-b6-observation-r1). This is not a Coherence verb and
# never counts toward F3: it ends the process that owns the window so the run
# can finish instead of idling to the watchdog, and reports what it did.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class BvClosureDiscard {
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
}
'@
$owner = [uint32]0
[void][BvClosureDiscard]::GetWindowThreadProcessId([IntPtr]$Hwnd, [ref]$owner)
if ($owner -eq 0) { Write-Output "BVDISCARD hwnd=$Hwnd pid=0 stopped=False"; exit 13 }
Stop-Process -Id $owner -Force -ErrorAction Stop
Start-Sleep -Seconds 1
Write-Output "BVDISCARD hwnd=$Hwnd pid=$owner stopped=$(-not [BvClosureDiscard]::IsWindow([IntPtr]$Hwnd))"
