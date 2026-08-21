[CmdletBinding()]
param(
    [int]$DurationMs = 20000,
    [int]$PollMs = 4
)

# B4 guest-side pointer instrument.
#
# The 2026-08-17 batch proved every host-visible counter is identical between a
# landed and a lost click, so the divergence is inside the guest. This probe
# reports whether the button transition ever reached the Windows input stack:
# GetAsyncKeyState(VK_LBUTTON) is polled in a tight loop -- deliberately
# callback-free, because native enumeration/hook callbacks hang the PS 5.1
# ARM64 host (live job 20260820-043633, WINLIST). Every line is grep-able and
# CRLF-carried into run.log by the agent.
#
# Interpretation contract:
#   BVPTR press/release seen   -> the HID report was consumed by the input
#                                 stack; a lost reaction is foreground-routing
#                                 or application-side.
#   BVPTR no transition        -> the loss is at or below the HID class driver;
#                                 next instrument is the xHCI transfer-ring
#                                 completion for the exact button TRB.

$ErrorActionPreference = 'Continue'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)]
public struct BvPtrPoint { public int X, Y; }
public static class BvPointerProbe {
  [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int key);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out BvPtrPoint point);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$down = $false
$presses = 0
$releases = 0
$firstPressMs = -1
$firstReleaseMs = -1
Write-Output ('BVPTR begin duration_ms=' + $DurationMs + ' poll_ms=' + $PollMs +
    ' utc=' + [DateTime]::UtcNow.ToString('o'))

while ($sw.ElapsedMilliseconds -lt $DurationMs) {
    $state = [BvPointerProbe]::GetAsyncKeyState(0x01)
    $now = ($state -band 0x8000) -ne 0
    if ($now -ne $down) {
        $pt = New-Object BvPtrPoint
        [void][BvPointerProbe]::GetCursorPos([ref]$pt)
        $fg = [BvPointerProbe]::GetForegroundWindow()
        $t = $sw.ElapsedMilliseconds
        if ($now) {
            $presses++
            if ($firstPressMs -lt 0) { $firstPressMs = $t }
            Write-Output ('BVPTR press t_ms=' + $t + ' x=' + $pt.X + ' y=' + $pt.Y + ' fg=' + $fg)
        } else {
            $releases++
            if ($firstReleaseMs -lt 0) { $firstReleaseMs = $t }
            Write-Output ('BVPTR release t_ms=' + $t + ' x=' + $pt.X + ' y=' + $pt.Y + ' fg=' + $fg)
        }
        $down = $now
    }
    Start-Sleep -Milliseconds $PollMs
}

Write-Output ('BVPTR summary presses=' + $presses + ' releases=' + $releases +
    ' first_press_ms=' + $firstPressMs + ' first_release_ms=' + $firstReleaseMs +
    ' stuck=' + (@{ $true = '1'; $false = '0' }[[bool]$down]))
exit 0
