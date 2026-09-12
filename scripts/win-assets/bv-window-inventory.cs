// Native callbacks remain entirely in compiled C#, never PowerShell scriptblocks.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;

namespace BridgeVM {
    public sealed class InventoryWindow {
        public string Handle;
        public uint ProcessId;
        public int X, Y, Width, Height;
        public string Title;
    }

    public static class NativeWindowInventory {
        public const int MaxRows = 4096, MaxBytes = 4194304, MaxVisits = 65536, MaxMillis = 2000;
        [StructLayout(LayoutKind.Sequential)]
        private struct Rect { public int Left, Top, Right, Bottom; }
        [UnmanagedFunctionPointer(CallingConvention.Winapi)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private delegate bool WindowCallback(IntPtr hwnd, IntPtr context);
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool EnumWindows(WindowCallback callback, IntPtr context);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindowVisible(IntPtr hwnd);
        [DllImport("user32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        private static extern int GetWindowTextW(IntPtr hwnd, StringBuilder text, int count);
        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetWindowRect(IntPtr hwnd, out Rect bounds);
        [DllImport("dwmapi.dll")]
        private static extern int DwmGetWindowAttribute(IntPtr hwnd, uint attribute, out Rect bounds, int size);

        private static InventoryWindow Read(IntPtr hwnd) {
            if (!IsWindowVisible(hwnd)) return null;
            var text = new StringBuilder(16385);
            int length = GetWindowTextW(hwnd, text, text.Capacity);
            if (length == 0) return null;
            if (length >= 16384) throw new InvalidOperationException("window title limit");
            string title = text.ToString();
            if (String.IsNullOrWhiteSpace(title)) return null;
            uint pid;
            if (GetWindowThreadProcessId(hwnd, out pid) == 0 || pid == 0) return null;
            Rect bounds;
            if (DwmGetWindowAttribute(hwnd, 9, out bounds, 16) != 0 && !GetWindowRect(hwnd, out bounds)) return null;
            long width = (long)bounds.Right - bounds.Left, height = (long)bounds.Bottom - bounds.Top;
            if (width <= 0 || height <= 0 || width > Int32.MaxValue || height > Int32.MaxValue) return null;
            long handle = hwnd.ToInt64();
            if (handle <= 0) throw new InvalidOperationException("invalid window handle");
            return new InventoryWindow { Handle = handle.ToString(CultureInfo.InvariantCulture),
                ProcessId = pid, X = bounds.Left, Y = bounds.Top, Width = (int)width, Height = (int)height, Title = title };
        }

        public static InventoryWindow[] Enumerate() {
            var rows = new List<InventoryWindow>();
            var handles = new HashSet<string>();
            var clock = Stopwatch.StartNew();
            var utf8 = new UTF8Encoding(false, true);
            int visits = 0, bytes = 0;
            Exception failure = null;
            WindowCallback callback = delegate(IntPtr hwnd, IntPtr context) {
                try {
                    if (++visits > MaxVisits || clock.ElapsedMilliseconds >= MaxMillis)
                        throw new InvalidOperationException("window enumeration visit/time limit");
                    InventoryWindow row = Read(hwnd);
                    if (row == null) return true;
                    bytes += 128 + ((utf8.GetByteCount(row.Title) + 2) / 3) * 4;
                    if (rows.Count >= MaxRows || bytes > MaxBytes || !handles.Add(row.Handle))
                        throw new InvalidOperationException("window enumeration row/byte/duplicate limit");
                    rows.Add(row);
                    return true;
                } catch (Exception error) {
                    failure = error;
                    return false;
                }
            };
            bool complete = EnumWindows(callback, IntPtr.Zero);
            int nativeError = Marshal.GetLastWin32Error();
            GC.KeepAlive(callback);
            if (failure != null) throw new InvalidOperationException("window enumeration incomplete", failure);
            if (!complete) throw new Win32Exception(nativeError);
            // Cooperative bound; an enclosing process deadline still owns a stuck native call.
            if (clock.ElapsedMilliseconds >= MaxMillis) throw new InvalidOperationException("window enumeration time limit");
            return rows.ToArray();
        }
    }
}
