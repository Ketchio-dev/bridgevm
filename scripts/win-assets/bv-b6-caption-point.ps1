param([long]$Hwnd, [int]$Width, [int]$Height)
$ErrorActionPreference = 'Stop'
if ($Hwnd -le 0 -or $Width -le 1 -or $Height -le 1) { throw 'Invalid target or display dimensions' }
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class BvCaptionPoint {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr hwnd, uint attribute, out Rect rect, uint size);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    [DllImport("user32.dll", SetLastError=true)] static extern IntPtr SendMessageTimeout(
        IntPtr hwnd, uint message, UIntPtr wparam, IntPtr lparam, uint flags, uint timeout, out UIntPtr result);
    public static string Find(long handle, int width, int height) {
        IntPtr hwnd = new IntPtr(handle);
        IntPtr previous = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (previous == IntPtr.Zero) throw new InvalidOperationException("Per-monitor physical coordinates unavailable");
        try {
            if (!IsWindowVisible(hwnd) || IsIconic(hwnd)) throw new InvalidOperationException("Target is absent, hidden or minimized");
            Rect rect;
            if (DwmGetWindowAttribute(hwnd, 9, out rect, 16) != 0 || rect.Right <= rect.Left || rect.Bottom <= rect.Top)
                throw new InvalidOperationException("No physical visible window bounds");
            uint dpi = GetDpiForWindow(hwnd);
            if (dpi == 0 || dpi > 960) throw new InvalidOperationException("Invalid window DPI");
            int bottom = Math.Min(rect.Bottom, rect.Top + (int)(64 * dpi / 96));
            int[] xs = { rect.Left + (rect.Right - rect.Left) / 2,
                         rect.Left + (rect.Right - rect.Left) / 3,
                         rect.Left + 2 * ((rect.Right - rect.Left) / 3) };
            foreach (int x in xs) {
                for (int y = rect.Top; y < bottom; y += 2) {
                    if (x < 0 || y < 0 || x >= width || y >= height || x > 32767 || y > 32767) continue;
                    Point point = new Point { X = x, Y = y };
                    IntPtr owner = GetAncestor(WindowFromPoint(point), 2);
                    if (owner != hwnd) continue;
                    UIntPtr hit;
                    IntPtr sent = SendMessageTimeout(hwnd, 0x84, UIntPtr.Zero,
                        new IntPtr((y << 16) | x), 0x22, 100, out hit);
                    if (sent == IntPtr.Zero) throw new InvalidOperationException("Caption hit-test timed out");
                    if (hit.ToUInt64() == 2) return String.Format(
                        "BVCAPTIONPOINT hwnd={0} x={1} y={2} dpi={3} hit=2 owner={0}", handle, x, y, dpi);
                }
            }
            throw new InvalidOperationException("No uncovered caption point owned by target");
        } finally { SetThreadDpiAwarenessContext(previous); }
    }
}
'@
[BvCaptionPoint]::Find($Hwnd, $Width, $Height)
