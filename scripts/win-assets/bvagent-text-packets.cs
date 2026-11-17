using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Security.Cryptography;
using System.Text;

namespace BridgeVM {
    // A carrier in the keyboard queue reaches the editor after earlier keys.
    // Its native callback dispatches the committed string on that editor thread.
    // A successful result proves window-message dispatch, not document mutation.
    public static class BvTextPackets {
        private const uint Magic = 0x42565431;
        private const int MappingBytes = 64 + 65536 * 2;
        private const string LibraryName = "bvagent-text-packets-arm64.dll";
        private static string cachedRoot, cachedLibrary;
        private static readonly object LibraryLock = new object();
        [StructLayout(LayoutKind.Sequential)] private struct SecurityAttributes {
            public int Size; public IntPtr Descriptor; public int Inherit;
        }
        [StructLayout(LayoutKind.Sequential)] private struct GuiInfo {
            public int Size, Flags;
            public IntPtr Active, Focus, Capture, Menu, Move, Caret;
            public int Left, Top, Right, Bottom;
        }
        [UnmanagedFunctionPointer(CallingConvention.Winapi)] private delegate int CancelPacket(IntPtr header);
        [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
        [DllImport("user32.dll")] private static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
        [DllImport("user32.dll")] private static extern bool IsWindowUnicode(IntPtr window);
        [DllImport("user32.dll")] private static extern short GetAsyncKeyState(int key);
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr word, IntPtr data, uint flags, uint timeout, out UIntPtr result);
        [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SetWindowsHookEx(int kind, IntPtr callback, IntPtr module, uint thread);
        [DllImport("user32.dll")] private static extern bool UnhookWindowsHookEx(IntPtr hook);
        [DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, [In] BvUnicodeInput.Input[] inputs, int size);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] private static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
        [DllImport("kernel32.dll", CharSet = CharSet.Ansi)] private static extern IntPtr GetProcAddress(IntPtr module, string name);
        [DllImport("kernel32.dll")] private static extern bool FreeLibrary(IntPtr module);
        [DllImport("kernel32.dll", SetLastError = true)] private static extern IntPtr OpenProcess(uint access, bool inherit, uint process);
        [DllImport("kernel32.dll")] private static extern bool IsWow64Process2(IntPtr process, out ushort machine, out ushort nativeMachine);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateFileMapping(IntPtr file, ref SecurityAttributes security, uint protection, uint high, uint low, string name);
        [DllImport("kernel32.dll")] private static extern IntPtr MapViewOfFile(IntPtr mapping, uint access, uint high, uint low, UIntPtr size);
        [DllImport("kernel32.dll")] private static extern bool UnmapViewOfFile(IntPtr view);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateEvent(ref SecurityAttributes security, bool manual, bool initial, string name);
        [DllImport("kernel32.dll")] private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
        [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
        [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool ConvertStringSecurityDescriptorToSecurityDescriptor(string sddl, uint revision, out IntPtr descriptor, out uint size);
        [DllImport("kernel32.dll")] private static extern IntPtr LocalFree(IntPtr memory);

        private static bool NativeArm64(IntPtr process) {
            ushort machine, nativeMachine;
            return IsWow64Process2(process, out machine, out nativeMachine) && machine == 0 && nativeMachine == 0xaa64;
        }

        private static string Hash(byte[] bytes) {
            using (var sha = SHA256.Create()) { return Convert.ToBase64String(sha.ComputeHash(bytes)); }
        }

        // The installed script directory is already protected by the agent installer.
        // Keep a separate immutable copy readable by this user's unelevated editor.
        // Loading a DLL directly from the user's writable share/temp would be unsafe.
        private static string ReadableLibrary(string root) {
            lock (LibraryLock) {
                if (cachedRoot == root && cachedLibrary != null) { return cachedLibrary; }
                string source = Path.Combine(root, LibraryName);
                byte[] bytes = File.ReadAllBytes(source);
                string path = source + ".readonly.dll";
                if (!File.Exists(path) || Hash(File.ReadAllBytes(path)) != Hash(bytes)) {
                    string temporary = path + "." + Guid.NewGuid().ToString("N");
                    try {
                        File.WriteAllBytes(temporary, bytes);
                        var acl = new FileSecurity();
                        var administrators = new SecurityIdentifier(WellKnownSidType.BuiltinAdministratorsSid, null);
                        var system = new SecurityIdentifier(WellKnownSidType.LocalSystemSid, null);
                        var user = WindowsIdentity.GetCurrent().User;
                        acl.SetOwner(administrators);
                        acl.SetAccessRuleProtection(true, false);
                        acl.AddAccessRule(new FileSystemAccessRule(administrators, FileSystemRights.FullControl, AccessControlType.Allow));
                        acl.AddAccessRule(new FileSystemAccessRule(system, FileSystemRights.FullControl, AccessControlType.Allow));
                        acl.AddAccessRule(new FileSystemAccessRule(user, FileSystemRights.ReadAndExecute, AccessControlType.Allow));
                        new FileInfo(temporary).SetAccessControl(acl);
                        if (File.Exists(path)) { File.Delete(path); }
                        File.Move(temporary, path);
                    } finally { if (File.Exists(temporary)) { File.Delete(temporary); } }
                }
                cachedRoot = root; cachedLibrary = path;
                return path;
            }
        }

        public static bool TryDispatch(string text, string sourceDirectory) {
            BvUnicodeInput.Build(text); // Validate the entire UTF-16 string before any side effect.
            if (Environment.OSVersion.Platform != PlatformID.Win32NT || IntPtr.Size != 8
                || String.IsNullOrEmpty(sourceDirectory) || !File.Exists(Path.Combine(sourceDirectory, LibraryName))) { return false; }
            IntPtr module = IntPtr.Zero, mapping = IntPtr.Zero, view = IntPtr.Zero, done = IntPtr.Zero;
            IntPtr hook = IntPtr.Zero, descriptor = IntPtr.Zero, targetProcess = IntPtr.Zero;
            bool injected = false;
            try {
                if (!NativeArm64(new IntPtr(-1)) || (GetAsyncKeyState(0x87) & 0x8000) != 0) { return false; }
                IntPtr foreground = GetForegroundWindow();
                uint foregroundProcess;
                uint thread = GetWindowThreadProcessId(foreground, out foregroundProcess);
                var gui = new GuiInfo(); gui.Size = Marshal.SizeOf(typeof(GuiInfo));
                if (foreground == IntPtr.Zero || !GetGUIThreadInfo(thread, ref gui) || gui.Focus == IntPtr.Zero || !IsWindowUnicode(gui.Focus)) { return false; }
                uint process;
                uint targetThread = GetWindowThreadProcessId(gui.Focus, out process);
                if (process != foregroundProcess) { return false; }
                targetProcess = OpenProcess(0x1000, false, process);
                if (targetProcess == IntPtr.Zero || !NativeArm64(targetProcess)) { return false; }
                UIntPtr support;
                if (SendMessageTimeout(gui.Focus, 0x109, new UIntPtr(0xffffu), IntPtr.Zero, 0x23, 500, out support) == IntPtr.Zero
                    || support.ToUInt64() != 1) { return false; }
                module = LoadLibraryEx(ReadableLibrary(sourceDirectory), IntPtr.Zero, 0x900);
                if (module == IntPtr.Zero) { return false; }
                IntPtr callback = GetProcAddress(module, "BridgeVMTextPacketHook");
                IntPtr cancelAddress = GetProcAddress(module, "BridgeVMCancelTextPacket");
                if (callback == IntPtr.Zero || cancelAddress == IntPtr.Zero) { return false; }
                var cancel = (CancelPacket)Marshal.GetDelegateForFunctionPointer(cancelAddress, typeof(CancelPacket));
                uint nonce;
                byte[] random = new byte[4];
                using (var rng = RandomNumberGenerator.Create()) { rng.GetBytes(random); }
                nonce = BitConverter.ToUInt32(random, 0);
                string name = "Local\\BridgeVM.TextPacket." + nonce.ToString("X8");
                string sid = WindowsIdentity.GetCurrent().User.Value;
                string sddl = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;" + sid + ")S:(ML;;NW;;;ME)";
                uint descriptorSize;
                if (!ConvertStringSecurityDescriptorToSecurityDescriptor(sddl, 1, out descriptor, out descriptorSize)) { return false; }
                var security = new SecurityAttributes(); security.Size = Marshal.SizeOf(typeof(SecurityAttributes)); security.Descriptor = descriptor;
                mapping = CreateFileMapping(new IntPtr(-1), ref security, 4, 0, MappingBytes, name);
                if (mapping == IntPtr.Zero || Marshal.GetLastWin32Error() == 183) { return false; }
                done = CreateEvent(ref security, true, false, name + ".done");
                if (done == IntPtr.Zero || Marshal.GetLastWin32Error() == 183) { return false; }
                view = MapViewOfFile(mapping, 6, 0, 0, new UIntPtr(MappingBytes));
                if (view == IntPtr.Zero) { return false; }
                Marshal.WriteInt32(view, 0, unchecked((int)Magic)); Marshal.WriteInt32(view, 4, unchecked((int)nonce));
                Marshal.WriteInt32(view, 8, text.Length); Marshal.WriteInt32(view, 12, unchecked((int)targetThread));
                Marshal.WriteInt64(view, 16, gui.Focus.ToInt64());
                byte[] utf16 = Encoding.Unicode.GetBytes(text); Marshal.Copy(utf16, 0, IntPtr.Add(view, 64), utf16.Length);
                hook = SetWindowsHookEx(3, callback, module, targetThread);
                if (hook == IntPtr.Zero) { return false; }
                var carrier = new BvUnicodeInput.Input(); carrier.Type = 1;
                carrier.Data.Keyboard.VirtualKey = 0x87; carrier.Data.Keyboard.Flags = 2;
                carrier.Data.Keyboard.ExtraInfo = new UIntPtr(((ulong)Magic << 32) | nonce);
                injected = true; // An exception crossing SendInput must not be treated as no dispatch.
                if (SendInput(1, new[] { carrier }, Marshal.SizeOf(typeof(BvUnicodeInput.Input))) != 1) { injected = false; return false; }
                uint waited = WaitForSingleObject(done, 20000);
                int state = Marshal.ReadInt32(view, 24);
                if (waited != 0 || state != 2) {
                    int previous = cancel(view);
                    if (previous == 0) { return false; } // Atomic cancellation proves text never started.
                    if (previous != 2) { throw new InvalidOperationException("unicode-packet-outcome-unknown state=" + previous
                        + " dispatched_units=" + Marshal.ReadInt32(view, 28) + " error=" + Marshal.ReadInt32(view, 32)); }
                }
                if (Marshal.ReadInt32(view, 28) != text.Length) { throw new InvalidOperationException("unicode-packet-dispatch-count"); }
                return true;
            } catch (Exception error) {
                if (injected) { throw new InvalidOperationException("unicode-packet-outcome-unknown", error); }
                return false;
            } finally {
                if (hook != IntPtr.Zero) { UnhookWindowsHookEx(hook); }
                if (view != IntPtr.Zero) { UnmapViewOfFile(view); }
                if (done != IntPtr.Zero) { CloseHandle(done); }
                if (mapping != IntPtr.Zero) { CloseHandle(mapping); }
                if (descriptor != IntPtr.Zero) { LocalFree(descriptor); }
                if (targetProcess != IntPtr.Zero) { CloseHandle(targetProcess); }
                if (module != IntPtr.Zero) { FreeLibrary(module); }
            }
        }
    }
}
