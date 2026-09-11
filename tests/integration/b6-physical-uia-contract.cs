using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class BvPhysicalUiaContract {
    [DllImport("user32.dll")] private static extern IntPtr GetThreadDpiAwarenessContext();
    [DllImport("user32.dll")] private static extern int GetAwarenessFromDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] private static extern bool AreDpiAwarenessContextsEqual(IntPtr first, IntPtr second);
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    public static object Verify() {
        int checks = 0;
        foreach (int context in new int[] { -1, -2, -4 }) {
            IntPtr previous = SetThreadDpiAwarenessContext(new IntPtr(context));
            if (previous == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                IntPtr caller = GetThreadDpiAwarenessContext();
                object result = BvPhysicalUia.Run(() => {
                    if (GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) != 2)
                        throw new Exception("Query did not enter per-monitor coordinates");
                    return "physical";
                });
                if (!object.Equals(result, "physical") || !AreDpiAwarenessContextsEqual(caller, GetThreadDpiAwarenessContext()))
                    throw new Exception("Success changed caller DPI context or result");
                checks++;
                bool caught = false;
                try { BvPhysicalUia.Run(() => { throw new InvalidOperationException("sentinel"); }); }
                catch (InvalidOperationException error) { caught = error.Message == "sentinel"; }
                if (!caught || !AreDpiAwarenessContextsEqual(caller, GetThreadDpiAwarenessContext()))
                    throw new Exception("Failure changed caller DPI context or exception");
                checks++;
            } finally {
                if (SetThreadDpiAwarenessContext(previous) == IntPtr.Zero)
                    throw new Win32Exception(Marshal.GetLastWin32Error());
            }
        }
        try { BvPhysicalUia.Run(null); throw new Exception("Null callback accepted"); }
        catch (ArgumentNullException) { checks++; }
        return checks;
    }
}
