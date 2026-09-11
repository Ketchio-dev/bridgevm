using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class BvNativeTipOwner {
    [StructLayout(LayoutKind.Sequential)] private struct Point { public int X, Y; }
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    public static object Visible(long window) { return IsWindowVisible(new IntPtr(window)); }
    public static object Process(long window) {
        uint process;
        if (GetWindowThreadProcessId(new IntPtr(window), out process) == 0) throw new InvalidOperationException("Window has no owning thread");
        return (long)process;
    }
    public static object At(int x, int y) {
        IntPtr previous = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (previous == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try { return GetAncestor(WindowFromPoint(new Point { X = x, Y = y }), 3).ToInt64(); }
        finally { SetThreadDpiAwarenessContext(previous); }
    }
}
