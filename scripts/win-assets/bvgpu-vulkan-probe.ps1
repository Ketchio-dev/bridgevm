[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$loader = Join-Path $env:windir 'System32\vulkan-1.dll'
$icdJson = 'C:\BridgeVM\viogpu3d\virtio_icd.arm64.json'
$icdDll = 'C:\BridgeVM\viogpu3d\vulkan_virtio.dll'
$probeLog = 'C:\BridgeVM\bvgpu-vulkan-probe.log'
$mesaDebugLog = 'C:\BridgeVM\bvgpu-mesa-debug.log'
$utf8NoBom = New-Object Text.UTF8Encoding($false)

function Write-Probe {
  param([Parameter(Mandatory = $true)][string]$Message)

  $line = '[vulkan-probe] ' + $Message
  # Open, append, and close on every boundary.  The stage-3 CMD log can remain
  # buffered while PowerShell or an ICD call is hung, so this file is the
  # authoritative last-completed-operation beacon after a watchdog stop.
  [IO.File]::AppendAllText($script:probeLog, $line + [Environment]::NewLine,
    $script:utf8NoBom)
  Write-Output $line
}

[IO.File]::WriteAllText($probeLog, '', $utf8NoBom)
[IO.File]::WriteAllText($mesaDebugLog, '', $utf8NoBom)
Write-Probe ('captured_utc=' + [DateTime]::UtcNow.ToString('o'))
Write-Probe ('loader=' + $loader + ' exists=' + (Test-Path -LiteralPath $loader -PathType Leaf))
if (Test-Path -LiteralPath $icdJson -PathType Leaf) {
  # Make the gate deterministic: do not let unrelated system/software ICDs
  # hide a Venus package failure or turn it into a misleading aggregate error.
  $env:VK_DRIVER_FILES = $icdJson
  $env:VK_LOADER_DEBUG = 'all'
}
Write-Probe ('driver_files=' + $env:VK_DRIVER_FILES)
Write-Probe ('loader_debug=' + $env:VK_LOADER_DEBUG)
$env:MESA_LOG = 'windbg'
$env:MESA_LOG_LEVEL = 'debug'
Write-Probe ('mesa_log=' + $env:MESA_LOG + ' level=' + $env:MESA_LOG_LEVEL)

$source = @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;

namespace BridgeVM {
  public static class KernelNative {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr LoadLibraryW(string lpLibFileName);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool FreeLibrary(IntPtr hLibModule);
  }

  // Mesa's Windows logger writes through OutputDebugString.  Receive the
  // documented DBWIN protocol in-process so the exact D3DKMT NTSTATUS remains
  // available on disk after the VM shuts down; no external debugger is needed.
  public static class DebugOutputCapture {
    const uint PAGE_READWRITE = 0x04;
    const uint FILE_MAP_READ = 0x0004;
    const uint WAIT_OBJECT_0 = 0;
    const uint WAIT_TIMEOUT = 258;
    static volatile bool stopping;
    static Thread worker;

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateEventW(IntPtr attributes, bool manualReset,
      bool initialState, string name);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateFileMappingW(IntPtr file, IntPtr attributes,
      uint protect, uint maximumSizeHigh, uint maximumSizeLow, string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr MapViewOfFile(IntPtr mapping, uint desiredAccess,
      uint fileOffsetHigh, uint fileOffsetLow, UIntPtr bytesToMap);

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

    static void Run(object state) {
      string path = (string)state;
      IntPtr ready = IntPtr.Zero;
      IntPtr data = IntPtr.Zero;
      IntPtr mapping = IntPtr.Zero;
      IntPtr view = IntPtr.Zero;
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
            string message = Marshal.PtrToStringAnsi(IntPtr.Add(view, 4)) ?? "";
            Append(path, "pid=" + pid + " " + message.TrimEnd('\r', '\n'));
          } else if (wait != WAIT_TIMEOUT) {
            Append(path, "wait_failed result=" + wait +
              " win32_error=" + Marshal.GetLastWin32Error());
            return;
          }
        }
      } catch (Exception ex) {
        Append(path, "exception=" + ex.GetType().FullName + " message=" + ex.Message);
      } finally {
        if (view != IntPtr.Zero) UnmapViewOfFile(view);
        if (mapping != IntPtr.Zero) CloseHandle(mapping);
        if (data != IntPtr.Zero) CloseHandle(data);
        if (ready != IntPtr.Zero) CloseHandle(ready);
      }
    }

    public static void Start(string path) {
      stopping = false;
      worker = new Thread(Run);
      worker.IsBackground = true;
      worker.Name = "BridgeVM DBWIN capture";
      worker.Start(path);
    }

    public static void Stop() {
      stopping = true;
      if (worker != null) worker.Join(1500);
    }
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct VkApplicationInfo {
    public uint sType;
    public IntPtr pNext;
    public IntPtr pApplicationName;
    public uint applicationVersion;
    public IntPtr pEngineName;
    public uint engineVersion;
    public uint apiVersion;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct VkInstanceCreateInfo {
    public uint sType;
    public IntPtr pNext;
    public uint flags;
    public IntPtr pApplicationInfo;
    public uint enabledLayerCount;
    public IntPtr ppEnabledLayerNames;
    public uint enabledExtensionCount;
    public IntPtr ppEnabledExtensionNames;
  }

  public static class VulkanNative {
    [DllImport("vulkan-1.dll", ExactSpelling = true)]
    public static extern int vkEnumerateInstanceVersion(out uint pApiVersion);

    [DllImport("vulkan-1.dll", ExactSpelling = true)]
    public static extern int vkCreateInstance(ref VkInstanceCreateInfo pCreateInfo,
      IntPtr pAllocator, out IntPtr pInstance);

    [DllImport("vulkan-1.dll", ExactSpelling = true)]
    public static extern int vkEnumeratePhysicalDevices(IntPtr instance,
      ref uint pPhysicalDeviceCount, IntPtr pPhysicalDevices);

    [DllImport("vulkan-1.dll", ExactSpelling = true)]
    public static extern void vkDestroyInstance(IntPtr instance, IntPtr pAllocator);
  }
}
'@

try {
  Write-Probe 'add_type_begin'
  Add-Type -TypeDefinition $source -Language CSharp
  Write-Probe 'add_type_end'
  [BridgeVM.DebugOutputCapture]::Start($mesaDebugLog)
  Start-Sleep -Milliseconds 300
  Write-Probe ('dbwin_capture_started log=' + $mesaDebugLog)
  Write-Probe ('direct_icd_load_begin path=' + $icdDll)
  $icdModule = [BridgeVM.KernelNative]::LoadLibraryW($icdDll)
  $icdLoadError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
  Write-Probe ('direct_icd_load_end module_nonzero=' + ($icdModule -ne [IntPtr]::Zero) +
    ' win32_error=' + $icdLoadError)
  if ($icdModule -ne [IntPtr]::Zero) {
    [void][BridgeVM.KernelNative]::FreeLibrary($icdModule)
  }
  $apiVersion = [uint32]0
  try {
    Write-Probe 'enumerate_instance_version_begin'
    $versionResult = [BridgeVM.VulkanNative]::vkEnumerateInstanceVersion([ref]$apiVersion)
    Write-Probe ('enumerate_instance_version_result=' + $versionResult +
      ' api_version=0x' + $apiVersion.ToString('x8'))
  } catch [EntryPointNotFoundException] {
    Write-Probe 'enumerate_instance_version=<loader-1.0>'
  }

  $appName = [Runtime.InteropServices.Marshal]::StringToHGlobalAnsi('BridgeVM Venus probe')
  try {
    $app = New-Object BridgeVM.VkApplicationInfo
    $app.sType = 0
    $app.pApplicationName = $appName
    $app.apiVersion = 0x00400000
    $appPtr = [Runtime.InteropServices.Marshal]::AllocHGlobal(
      [Runtime.InteropServices.Marshal]::SizeOf($app))
    try {
      [Runtime.InteropServices.Marshal]::StructureToPtr($app, $appPtr, $false)
      $create = New-Object BridgeVM.VkInstanceCreateInfo
      $create.sType = 1
      $create.pApplicationInfo = $appPtr
      $instance = [IntPtr]::Zero
      Write-Probe 'create_instance_begin'
      $createTimer = [Diagnostics.Stopwatch]::StartNew()
      $createResult = [BridgeVM.VulkanNative]::vkCreateInstance([ref]$create,
        [IntPtr]::Zero, [ref]$instance)
      $createTimer.Stop()
      Write-Probe ('create_instance_result=' + $createResult +
        ' instance_nonzero=' + ($instance -ne [IntPtr]::Zero) +
        ' elapsed_ms=' + $createTimer.ElapsedMilliseconds)
      if ($createResult -ne 0 -or $instance -eq [IntPtr]::Zero) { exit 10 }
      try {
        $physicalDeviceCount = [uint32]0
        Write-Probe 'enumerate_physical_devices_begin'
        $enumerateResult = [BridgeVM.VulkanNative]::vkEnumeratePhysicalDevices(
          $instance, [ref]$physicalDeviceCount, [IntPtr]::Zero)
        Write-Probe ('enumerate_physical_devices_result=' + $enumerateResult +
          ' count=' + $physicalDeviceCount)
        if ($enumerateResult -ne 0) { exit 11 }
        if ($physicalDeviceCount -lt 1) { exit 12 }
      } finally {
        Write-Probe 'destroy_instance_begin'
        [BridgeVM.VulkanNative]::vkDestroyInstance($instance, [IntPtr]::Zero)
        Write-Probe 'destroy_instance_end'
      }
    } finally {
      [Runtime.InteropServices.Marshal]::FreeHGlobal($appPtr)
    }
  } finally {
    [Runtime.InteropServices.Marshal]::FreeHGlobal($appName)
  }
} catch {
  Write-Probe ('exception=' + $_.Exception.GetType().FullName +
    ' message=' + $_.Exception.Message)
  exit 20
}

Write-Probe 'success'
exit 0
