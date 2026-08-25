[CmdletBinding()]
param([ValidateRange(1000, 900000)][int]$DurationMs = 900000)

$ErrorActionPreference = 'Stop'
$OutputPath = 'C:\BridgeVMPtr\b4-dbwin.log'
$ReadyPath = 'C:\BridgeVMPtr\b4-dbwin-ready.log'
$CompletePath = 'C:\BridgeVMPtr\b4-dbwin-complete.log'
$StopPath = 'C:\BridgeVMPtr\b4-dbwin-stop.request'
foreach ($path in @($OutputPath, $ReadyPath, $CompletePath, $StopPath)) {
  Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
}

Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace BridgeVMB4 {
  public static class BoundedDebugOutputCapture {
    const uint PAGE_READWRITE = 0x04;
    const uint FILE_MAP_READ = 0x0004;
    const uint WAIT_OBJECT_0 = 0;
    const uint WAIT_TIMEOUT = 258;
    const int MAX_LINES = 8192;
    static volatile bool stopping;
    static Thread worker;
    static int retained;

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateEventW(IntPtr attributes, bool manualReset,
      bool initialState, string name);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateFileMappingW(IntPtr file, IntPtr attributes,
      uint protect, uint high, uint low, string name);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr MapViewOfFile(IntPtr mapping, uint access,
      uint high, uint low, UIntPtr bytes);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool SetEvent(IntPtr handle);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool UnmapViewOfFile(IntPtr address);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool CloseHandle(IntPtr handle);

    static void Append(string path, string message) {
      File.AppendAllText(path, "[dbwin] " + message + Environment.NewLine);
    }

    static void Retain(string path, int pid, string message) {
      if (!message.StartsWith("BV-VIRGL-", StringComparison.Ordinal)) return;
      int line = Interlocked.Increment(ref retained);
      if (line > MAX_LINES) {
        if (line == MAX_LINES + 1) Append(path, "line_limit_reached=" + MAX_LINES);
        return;
      }
      message = message.TrimEnd('\r', '\n');
      if (message.Length > 480) message = message.Substring(0, 480);
      Append(path, "pid=" + pid + " " + message);
    }

    static void Run(object state) {
      string path = (string)state;
      IntPtr ready = IntPtr.Zero, data = IntPtr.Zero;
      IntPtr mapping = IntPtr.Zero, view = IntPtr.Zero;
      try {
        ready = CreateEventW(IntPtr.Zero, false, false, "DBWIN_BUFFER_READY");
        data = CreateEventW(IntPtr.Zero, false, false, "DBWIN_DATA_READY");
        mapping = CreateFileMappingW(new IntPtr(-1), IntPtr.Zero,
          PAGE_READWRITE, 0, 4096, "DBWIN_BUFFER");
        if (ready == IntPtr.Zero || data == IntPtr.Zero || mapping == IntPtr.Zero) {
          Append(path, "init_failed win32_error=" + Marshal.GetLastWin32Error());
          return;
        }
        view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, new UIntPtr(4096));
        if (view == IntPtr.Zero) {
          Append(path, "map_failed win32_error=" + Marshal.GetLastWin32Error());
          return;
        }
        Append(path, "capture_ready");
        while (!stopping) {
          SetEvent(ready);
          uint wait = WaitForSingleObject(data, 250);
          if (wait == WAIT_OBJECT_0) {
            int pid = Marshal.ReadInt32(view);
            byte[] bytes = new byte[4092];
            Marshal.Copy(IntPtr.Add(view, 4), bytes, 0, bytes.Length);
            int length = Array.IndexOf(bytes, (byte)0);
            if (length < 0) length = bytes.Length;
            string message = Encoding.Default.GetString(bytes, 0, length);
            Retain(path, pid, message);
          } else if (wait != WAIT_TIMEOUT) {
            Append(path, "wait_failed result=" + wait +
              " win32_error=" + Marshal.GetLastWin32Error());
            return;
          }
        }
      } catch (Exception error) {
        Append(path, "exception=" + error.GetType().FullName);
      } finally {
        if (view != IntPtr.Zero) UnmapViewOfFile(view);
        if (mapping != IntPtr.Zero) CloseHandle(mapping);
        if (data != IntPtr.Zero) CloseHandle(data);
        if (ready != IntPtr.Zero) CloseHandle(ready);
      }
    }

    public static void Start(string path) {
      stopping = false;
      retained = 0;
      worker = new Thread(Run);
      worker.IsBackground = true;
      worker.Name = "BridgeVM B4 DBWIN capture";
      worker.Start(path);
    }

    public static void Stop() {
      stopping = true;
      if (worker != null) worker.Join(2000);
    }
  }
}
'@

try {
  [BridgeVMB4.BoundedDebugOutputCapture]::Start($OutputPath)
  $readyDeadline = [DateTime]::UtcNow.AddSeconds(10)
  while ([DateTime]::UtcNow -lt $readyDeadline) {
    if ((Test-Path -LiteralPath $OutputPath) -and
        (Select-String -LiteralPath $OutputPath -SimpleMatch 'capture_ready' -Quiet)) { break }
    Start-Sleep -Milliseconds 100
  }
  if (-not (Test-Path -LiteralPath $OutputPath) -or
      -not (Select-String -LiteralPath $OutputPath -SimpleMatch 'capture_ready' -Quiet)) {
    throw 'DBWIN capture did not become ready'
  }
  Set-Content -LiteralPath $ReadyPath -Value 'B4DBWIN ready=true' -Encoding Ascii
  $deadline = [DateTime]::UtcNow.AddMilliseconds($DurationMs)
  while ([DateTime]::UtcNow -lt $deadline -and -not (Test-Path -LiteralPath $StopPath)) {
    Start-Sleep -Milliseconds 250
  }
  [BridgeVMB4.BoundedDebugOutputCapture]::Stop()
  Set-Content -LiteralPath $CompletePath -Value 'B4DBWIN complete=true' -Encoding Ascii
} catch {
  [BridgeVMB4.BoundedDebugOutputCapture]::Stop()
  Set-Content -LiteralPath $CompletePath -Value ('B4DBWIN complete=false error=' +
    $_.Exception.GetType().FullName) -Encoding Ascii
  exit 1
}
