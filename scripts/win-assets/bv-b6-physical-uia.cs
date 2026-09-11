using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class BvPhysicalUia {
    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    public static object Query(long window) {
        return Run(() => BridgeVmNativeUia.Probe.Query(window));
    }
    public static object Run(Func<object> query) {
        if (query == null) throw new ArgumentNullException("query");
        IntPtr previous = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (previous == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try { return query(); }
        finally {
            if (SetThreadDpiAwarenessContext(previous) == IntPtr.Zero)
                throw new Win32Exception(Marshal.GetLastWin32Error());
        }
    }
}
