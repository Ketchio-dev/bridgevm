# BridgeVM guest agent - runs host commands over the virtio-serial port.
# No compiled binary: PowerShell + kernel32/cfgmgr32 P/Invoke. Uses RAW IntPtr
# handles (SafeFileHandle mis-reports INVALID_HANDLE_VALUE as valid on
# Win-ARM64 / PS 5.1) with a manual INVALID_HANDLE_VALUE check. The port is
# opened by discovering the vioser device-interface path (GUID_VIOSERIAL_PORT),
# not a fixed \\.\Global\<name> symlink (that symlink only exists if PORT_NAME
# was processed, and was failing with GetLastError=203). Line protocol (ASCII,
# \n-terminated):
#   host->guest:  RUN <base64(utf8 cmd)>\n | PS <base64(utf8 script)>\n | PING\n
#   guest->host:  READY <hostname>\n | OUT <exit> <base64(utf8 out)>\n |
#                 OUTBEG <exit> <bytes> <chunks>\n |
#                 OUTCHUNK <seq> <base64(raw)>\n | OUTEND <chunks>\n | PONG\n
# Reader note: PowerShell 5.1 P/Invoke marshaling is very expensive per call,
# so reading one byte at a time makes multi-KB CLIPSET / PUT protocol lines
# stall the channel for tens of seconds. Read-Line therefore reads up to 16 KiB
# per ReadFile call, decodes ASCII slices natively, queues complete LF-terminated
# lines, and keeps a trailing partial line across idle polls.
$ErrorActionPreference = 'Continue'

$bvLog = 'C:\bvagent.log'
function Log([string]$m) {
    $ts = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss.fff')
    try { Add-Content -LiteralPath $bvLog -Value "$ts pid=$PID $m" -ErrorAction SilentlyContinue } catch { }
}
# The log keeps its last few MB: one past 8 MB moves to bvagent.log.old at start.
try {
    if ((Get-Item -LiteralPath $bvLog -ErrorAction Stop).Length -gt 8MB) {
        Move-Item -LiteralPath $bvLog -Destination ($bvLog + '.old') -Force -ErrorAction Stop
    }
} catch { }

# Single-instance guard, decided BEFORE any port access. A duplicate exits
# without opening; the winner opens once and never exits, holding the mutex and
# the port for the session. (.NET doesn't grant ownership when merely opening an
# existing named mutex, so a duplicate has nothing to release.)
$bvCreated = $false
$bvMutex = New-Object System.Threading.Mutex($true, 'Global\BridgeVMAgentSingleton', [ref]$bvCreated)
Log "AGENT START (singleton) created=$bvCreated"
if (-not $bvCreated) {
    Log 'AGENT DUPLICATE - exiting without opening the port'
    exit 0
}
$script:BvMutexKeepAlive = $bvMutex

# vioser port device-interface class GUID (public.h GUID_VIOSERIAL_PORT).
$PortGuid = [System.Guid]'6FDE7521-1B65-48AE-B628-80BE62016026'
# IOCTL_GET_INFORMATION_BUFFERED: CTL_CODE(FILE_DEVICE_UNKNOWN=0x22, 0x800,
# METHOD_BUFFERED=0, FILE_ANY_ACCESS=0). Returns VIRTIO_PORT_INFO
# { UINT Id; BOOLEAN OutVqFull; BOOLEAN HostConnected; BOOLEAN GuestConnected;
#   CHAR Name[1]; } - default packing: Id@0-3, OutVqFull@4, HostConnected@5,
# GuestConnected@6, Name@7.
$IOCTL_GET_INFORMATION_BUFFERED = 0x222000

$sig = @'
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
public static extern IntPtr CreateFile(string n, uint access, uint share, IntPtr sa, uint disp, uint flags, IntPtr templ);
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool ReadFile(IntPtr h, byte[] buf, uint toRead, out uint read, IntPtr overlapped);
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool WriteFile(IntPtr h, byte[] buf, uint toWrite, out uint written, IntPtr overlapped);
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool DeviceIoControl(IntPtr h, uint code, IntPtr inBuf, uint inSize, byte[] outBuf, uint outSize, out uint returned, IntPtr overlapped);
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool CloseHandle(IntPtr h);
[DllImport("cfgmgr32.dll", EntryPoint="CM_Get_Device_Interface_List_SizeW", CharSet=CharSet.Unicode)]
public static extern int CM_Get_Device_Interface_List_Size(out uint size, ref System.Guid guid, IntPtr deviceID, uint flags);
[DllImport("cfgmgr32.dll", EntryPoint="CM_Get_Device_Interface_ListW", CharSet=CharSet.Unicode)]
public static extern int CM_Get_Device_Interface_List(ref System.Guid guid, IntPtr deviceID, byte[] buffer, uint bufferLen, uint flags);
[DllImport("user32.dll")]
public static extern uint GetClipboardSequenceNumber();
'@
$K = Add-Type -MemberDefinition $sig -Name 'BvKernel32' -Namespace 'BridgeVM' -PassThru

# Keep the already-live-proven channel interop above isolated from the optional
# window API. After the nested delegate/struct declarations were added to
# -MemberDefinition, the live agent produced no return value from its native
# calls and even the CreateFile(ntdll.dll) sanity probe failed before it could
# open the virtio-serial port. Keep a complete window type definition outside
# that measured failure boundary without changing the channel type's shape.
$windowSig = @'
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace BridgeVM {
    public static class BvWindow {
        [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out BvRect rect, int size);
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out BvRect rect);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int w, int h, uint flags);
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
        [StructLayout(LayoutKind.Sequential)] public struct BvRect { public int Left, Top, Right, Bottom; }
    }
}
'@
$W = $null
try {
    Add-Type -TypeDefinition $windowSig -ErrorAction Stop
    $W = [BridgeVM.BvWindow]
} catch {
    Log ("WINDOW API init FAILED: " + $_.Exception.Message)
}

# Optional inventory support must not break the resident command channel.
$bvInventoryErrorAction = $ErrorActionPreference
try {
    $ErrorActionPreference = 'Stop'
    . (Join-Path $PSScriptRoot 'bvagent-window-inventory.ps1')
    if ($null -eq (Get-Command Get-BvWindowInventoryLines -CommandType Function -ErrorAction SilentlyContinue)) {
        throw 'window-inventory-function-missing'
    }
} catch {
    Log ('WINDOW INVENTORY unavailable: ' + $_.Exception.Message)
    function Get-BvWindowInventoryLines { throw 'window-inventory-unavailable' }
} finally {
    $ErrorActionPreference = $bvInventoryErrorAction
}

# Optional input support must not break the resident command channel.
try {
    . (Join-Path $PSScriptRoot 'bvagent-input.ps1')
} catch {
    Log ('INPUT API unavailable: ' + $_.Exception.Message)
    function Invoke-UnicodeInput([string]$Request, [scriptblock]$Sender = $null, [bool]$KeyInput = $false) {
        return @{ Exit = 1; Out = 'BVINPUT_FAILED unavailable' }
    }
}

function Get-InputCapabilities([string]$Request) {
    try {
        $probe = Invoke-UnicodeInput ($Request + ' YQ==') { param($text) [uint32]2 }
        if ($probe.Exit -ne 0 -or [BridgeVM.BvUnicodeInput]::BuildKey('enter').Length -ne 2 -or [BridgeVM.BvUnicodeInput]::BuildPointer('click:0x0').Length -ne 2 -or [BridgeVM.BvUnicodeInput]::BuildPointer('wheel:-1').Length -ne 1 -or [BridgeVM.BvUnicodeInput]::BuildPointer('scroll:-128@32767x0').Length -ne 2 -or [BridgeVM.BvUnicodeInput]::BuildPointer('releaseall:0x0')[0].Data.Mouse.Flags -ne 0x8015) {
            throw 'input-support-unavailable'
        }
        $keySupport = ''
        # A running older C# module remains valid v3; advertise only keys it can build.
        try {
            if ([BridgeVM.BvUnicodeInput]::BuildKey('shift+enter').Length -eq 4 -and [BridgeVM.BvUnicodeInput]::BuildKey('shift+space').Length -eq 4) {
                $keySupport = "`nBVINPUT_KEYS $Request SHIFT_ENTER_SPACE"
            }
        } catch { }
        # Hosts that predate the separate chord marker ignore it. Advertise it only when every chord dry-builds.
        try {
            $chords = @([char[]]'abcdefghijklmnopqrstuvwxyz' | ForEach-Object { 'ctrl+' + $_ }) + @(1..12 | ForEach-Object { 'shift+f' + $_ }) + @('ctrl+enter', 'ctrl+tab', 'shift+delete')
            foreach ($chord in $chords) { if ([BridgeVM.BvUnicodeInput]::BuildKey($chord).Length -ne 4) { throw 'chord-unavailable' } }
            if ([BridgeVM.BvUnicodeInput]::BuildKey('ctrl+shift+tab').Length -eq 6) { $keySupport += "`nBVINPUT_CHORDS $Request CTRL_SHIFT_CHORDS" }
        } catch { }
        # Any modifiers plus a key, and a bare modifier tap; advertised only when every probe dry-builds.
        try {
            foreach ($chord in @('win', 'alt', 'win+e', 'win+shift+s', 'alt+f', 'ctrl+1', 'ctrl+slash', 'ctrl+shift+esc', 'win+ctrl+alt+shift+f12')) {
                if ([BridgeVM.BvUnicodeInput]::BuildKey($chord).Length -ne 2 * $chord.Split('+').Length) { throw 'modifier-chord-unavailable' }
            }
            $keySupport += "`nBVINPUT_CHORDS $Request MODIFIER_CHORDS"
        } catch { }
        return @{ Exit = 0; Out = (('BVINPUT_CAPS {0} 3 TEXTINPUT KEYINPUT POINTERINPUT 65536' -f $Request) + $keySupport) }
    } catch {
        return @{ Exit = 1; Out = 'BVINPUT_CAPS_FAILED unavailable' }
    }
}

function Gle { return [System.Runtime.InteropServices.Marshal]::GetLastWin32Error() }

# CreateFile access rights as EXPLICIT [uint32] decimals. PowerShell 5.1 parses
# the hex literals 0x80000000 / 0xC0000000 as a NEGATIVE [int] (int-min etc.),
# and binding a negative int to the P/Invoke's `uint access` parameter mangles
# the value so CreateFile fails for EVERYTHING (even \\.\C:) with a bogus/stale
# GetLastError. Decimal [uint32] literals avoid the signed-hex trap entirely.
$GENERIC_READ = [uint32]2147483648   # 0x80000000
$GENERIC_RW = [uint32]3221225472     # 0xC0000000 (GENERIC_READ | GENERIC_WRITE)
$FILE_SHARE_RW = [uint32]3           # FILE_SHARE_READ | FILE_SHARE_WRITE
$OPEN_EXISTING = [uint32]3

function Is-BadHandle($handle) {
    if ($null -eq $handle) { return $true }
    $v = $handle.ToInt64()
    return ($v -eq 0) -or ($v -eq -1)
}

# Enumerate present vioser port device-interface paths via cfgmgr32 (returns a
# REG_MULTI_SZ of \\?\... paths; simpler than SetupDi* - no interop structs).
function Get-PortInterfacePaths {
    $result = @()
    $g = $PortGuid
    $size = [uint32]0
    $cr = $K::CM_Get_Device_Interface_List_Size([ref]$size, [ref]$g, [IntPtr]::Zero, 0)
    if ($cr -ne 0) { Log "CM_ListSize cr=$cr"; return $result }
    if ($size -le 1) { Log "CM_ListSize empty size=$size"; return $result }
    $bytes = New-Object byte[] ([int]$size * 2)
    $cr2 = $K::CM_Get_Device_Interface_List([ref]$g, [IntPtr]::Zero, $bytes, $size, 0)
    if ($cr2 -ne 0) { Log "CM_List cr=$cr2"; return $result }
    $joined = [System.Text.Encoding]::Unicode.GetString($bytes)
    foreach ($p in $joined.Split([char]0)) {
        if (-not [string]::IsNullOrWhiteSpace($p)) { $result += $p }
    }
    return $result
}

# Query vioser's VIRTIO_PORT_INFO for an open handle. Returns a hashtable or
# $null. Works right after CreateFile (the ioctl queue is up once the PDO is).
function Get-PortInfoRaw($handle) {
    if (Is-BadHandle $handle) { return $null }
    $buf = New-Object byte[] 256
    $ret = [uint32]0
    $ok = $K::DeviceIoControl($handle, [uint32]$IOCTL_GET_INFORMATION_BUFFERED, [IntPtr]::Zero, 0, $buf, [uint32]$buf.Length, [ref]$ret, [IntPtr]::Zero)
    if (-not $ok -or $ret -lt 7) { return $null }
    $id = [System.BitConverter]::ToUInt32($buf, 0)
    $name = ''
    if ($ret -gt 7) {
        $end = 7
        while ($end -lt $ret -and $buf[$end] -ne 0) { $end++ }
        if ($end -gt 7) { $name = [System.Text.Encoding]::ASCII.GetString($buf, 7, $end - 7) }
    }
    return @{ Id = $id; OutVqFull = $buf[4]; Host = $buf[5]; Guest = $buf[6]; Name = $name }
}

# Idle polls (every ~2 s) log the port state only when it changed, so a
# quiet agent adds no lines; a line each poll was ~43k lines a day.
function Log-PortInfo($handle, [string]$when) {
    $info = Get-PortInfoRaw $handle
    $state = if ($null -eq $info) { 'unavailable' } else { "id=$($info.Id) HostConnected=$($info.Host) GuestConnected=$($info.Guest) OutVqFull=$($info.OutVqFull) name='$($info.Name)'" }
    if ($when -eq 'idle' -and $state -eq $script:LastPortState) { return }
    $script:LastPortState = $state
    Log "PORTINFO[$when] $state"
}

# Discover and open the agent port. Prefer the enumerated device-interface whose
# vioser port Id is 1 (our agent port) or whose Name matches; fall back to the
# name symlinks. Logs every candidate + result so the log reveals the real path.
function Try-OpenPort {
    foreach ($path in @(Get-PortInterfacePaths)) {
        $handle = $K::CreateFile($path, $GENERIC_RW, $FILE_SHARE_RW, [IntPtr]::Zero, $OPEN_EXISTING, 0, [IntPtr]::Zero)
        $gle = Gle
        if (Is-BadHandle $handle) { Log "PORT try iface FAILED gle=$gle ($(Win32Msg $gle)) path=$path"; continue }
        $info = Get-PortInfoRaw $handle
        if ($null -ne $info) {
            Log "PORT try iface OPENED id=$($info.Id) name='$($info.Name)' Host=$($info.Host) path=$path"
            if ($info.Id -eq 1 -or $info.Name -eq 'org.bridgevm.agent.0') {
                Log "PORT PATH FOUND $path"
                return $handle
            }
        } else {
            Log "PORT try iface OPENED (no info) path=$path"
        }
        [void]$K::CloseHandle($handle)
    }
    foreach ($path in @('\\.\Global\org.bridgevm.agent.0', '\\.\org.bridgevm.agent.0')) {
        $handle = $K::CreateFile($path, $GENERIC_RW, $FILE_SHARE_RW, [IntPtr]::Zero, $OPEN_EXISTING, 0, [IntPtr]::Zero)
        $gle = Gle
        if (Is-BadHandle $handle) { Log "PORT try symlink FAILED gle=$gle ($(Win32Msg $gle)) path=$path"; continue }
        Log "PORT PATH FOUND $path"
        return $handle
    }
    return ([System.IntPtr]::Zero)
}

# vioser hands each WriteFile to the device as one transfer. A whole 4 MiB
# clipboard reply (a 5.6 MB line) never left the guest, and hosts before
# 2026-11-07 dropped any write over 64 KiB, a 48 KiB clipboard, so the channel
# waited for that reply forever. Long lines therefore go out in 32 KiB writes;
# the host joins them at the newline.
function Write-Bytes($handle, [byte[]]$data, [string]$what) {
    if (Is-BadHandle $handle) { Log "WriteFile[$what] SKIPPED bad handle"; return }
    $want = $data.Length
    $done = 0
    while ($done -lt $want) {
        $count = [Math]::Min(32768, $want - $done)
        $buf = $data
        if ($done -ne 0 -or $count -ne $want) {
            if ($null -eq $script:BvWriteSlice) { $script:BvWriteSlice = New-Object byte[] 32768 }
            [System.Array]::Copy($data, $done, $script:BvWriteSlice, 0, $count)
            $buf = $script:BvWriteSlice
        }
        $written = [uint32]0
        $ok = $K::WriteFile($handle, $buf, [uint32]$count, [ref]$written, [IntPtr]::Zero)
        $gle = Gle
        if (-not $ok -or $written -eq 0) {
            Log "WriteFile[$what] FAILED ok=$ok written=$written total=$done want=$want gle=$gle"
            return
        }
        # A synchronous WriteFile may complete only a prefix; the loop writes the
        # rest, since a missing newline leaves the host waiting on a partial frame.
        $done += [int]$written
    }
    # Only failures are logged: a line per message (every PONG, clipboard and
    # share poll) grew C:\bvagent.log by about 130k lines a day.
}
function Write-Line($handle, [string]$line, [string]$what) {
    Write-Bytes $handle ([System.Text.Encoding]::ASCII.GetBytes($line + "`n")) $what
}

$script:BvReadBuffer = New-Object byte[] 16384
$script:BvPending = New-Object System.Text.StringBuilder
$script:BvLines = New-Object 'System.Collections.Generic.Queue[string]'
# CLIPGET cache: only re-open the clipboard when the sequence number moved.
# 4294967295 = [uint32]::MaxValue sentinel so the first CLIPGET always reads.
$script:BvClipSeq = [uint32]4294967295
$script:BvClipCache = ''
Log 'Read-Line buffered reader init size=16384'

$script:BvPutStream = $null
$script:BvPutPath = ''
$script:BvPutTotal = [int64]0
$script:BvPutWritten = [int64]0
$script:BvPutSeq = 0

function Get-IoPath([string]$path, [bool]$always = $false) {
    # Win32 refuses a path of 260 characters or more (a folder from 248) unless it starts with
    # \\?\, which .NET 4.6.2 and later accept. \\?\ also skips Win32's name rules (trailing dots
    # and spaces trimmed), which the host's name rules expect, so the path gets them from
    # GetFullPath first. Shorter paths stay as they are.
    if ((-not $always -and $path.Length -lt 248) -or $path.StartsWith('\\?\')) { return $path }
    $full = [System.IO.Path]::GetFullPath($path)
    if ($full -match '^[A-Za-z]:\\') { return '\\?\' + $full }
    if ($full.StartsWith('\\')) { return '\\?\UNC\' + $full.Substring(2) }
    return $path
}

function Set-PutTime([string]$path, [string]$ms) {
    # The Mac file's modification time (ms since 1970) when the host sends one; only the
    # date Explorer shows, so a failure leaves the written file as it is.
    if (-not $ms) { return }
    try { [System.IO.File]::SetLastWriteTimeUtc((Get-IoPath $path), [DateTimeOffset]::FromUnixTimeMilliseconds([long]$ms).UtcDateTime) } catch { }
}

function Close-PutStream {
    if ($null -ne $script:BvPutStream) {
        try { $script:BvPutStream.Close() } catch { }
        $script:BvPutStream = $null
    }
    $script:BvPutPath = ''
    $script:BvPutTotal = [int64]0
    $script:BvPutWritten = [int64]0
    $script:BvPutSeq = 0
    $script:BvPutTime = ''
}

# >>> large put
# A large shared-folder file from the Mac (LPUTBEG, LPUTDATA, LPUTEND, LPUTABORT) is written into a
# temporary of its own beside the target, apart from PUTBEG's stream, so clipboard images and tools
# updates can go between its chunks. At the end it is flushed to disk and renamed over the target in
# one step, so the name holds the old file or the whole new one, never part of one. An error or an
# abort deletes the temporary. Its name is exactly .bridgevm-part-<32 lowercase hex>; LSR never lists
# such a file and deletes one no PUT holds once it is a few minutes old (an earlier agent's). Any
# other name, a user's .bridgevm-part-notes too, is a file like any other.
$script:BvLPut = $null

function Close-LargePut {
    $put = $script:BvLPut
    $script:BvLPut = $null
    if ($null -eq $put) { return }
    try { $put.Stream.Close() } catch { }
    try { [System.IO.File]::Delete($put.Temp) } catch { }
}

function Get-PutError($err) {
    # A full disk (ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL) as the one word the host reports.
    $e = $err.Exception
    while ($null -ne $e) {
        if ($e.HResult -eq -2147024784 -or $e.HResult -eq -2147024857) { return 'disk-full' }
        $e = $e.InnerException
    }
    return $err.Exception.Message
}

function Move-Replace([string]$from, [string]$to) {
    # MoveFileEx with REPLACE_EXISTING and WRITE_THROUGH: one rename. .NET Framework's File.Move
    # cannot replace a file.
    if ([System.IO.Path]::DirectorySeparatorChar -eq '\' -and (Test-DropReader)) { [BridgeVM.BvDropReader]::Replace($from, $to); return }
    if ([System.IO.File]::Exists($to)) { [System.IO.File]::Replace($from, $to, [NullString]::Value) } else { [System.IO.File]::Move($from, $to) }
}
# <<< large put

# Finder drops land on the logged-on user's Desktop through their own stream, apart from the
# shared folder's PUT stream, so share sync and a drop can take turns on the wire.
$script:BvDropStream = $null
$script:BvDropPath = ''
$script:BvDropTemp = ''
$script:BvDropTotal = [int64]0
$script:BvDropWritten = [int64]0
$script:BvDropSeq = [int64]0

function Close-DropStream([bool]$Keep) {
    if ($null -ne $script:BvDropStream) {
        try { $script:BvDropStream.Close() } catch { }
        $script:BvDropStream = $null
    }
    if (-not $Keep -and $script:BvDropTemp) { try { [System.IO.File]::Delete($script:BvDropTemp) } catch { } }
    $script:BvDropTemp = ''
    $script:BvDropPath = ''
    $script:BvDropTotal = [int64]0
    $script:BvDropWritten = [int64]0
    $script:BvDropSeq = [int64]0
}

# A 1 MiB chunk arrives as about 340 vioser receive buffers (one per ReadFile call). Interpreted,
# that loop is the transfer's ceiling, so the payload line has its own compiled reader. It is
# compiled on the first DROPCAPS, not at agent start, so boot does not pay for it.
$script:BvDropReader = $null
$script:BvDropReaderSource = @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
namespace BridgeVM {
    public static class BvDropReader {
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool ReadFile(IntPtr h, byte[] buf, uint toRead, out uint read, IntPtr overlapped);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        static extern bool MoveFileEx(string from, string to, uint flags);
        static readonly byte[] Scratch = new byte[65536];

        // MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH: `to` is the old file or `from`, whole.
        public static void Replace(string from, string to) {
            if (!MoveFileEx(from, to, 9)) { throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
        }
        public static long ReadTicks;
        public static long DecodeTicks;

        // Completes one base64 payload line of exactly `want` characters plus its newline, decoding
        // each read as it arrives: Convert.FromBase64String over the whole line was more than half
        // of a chunk's guest time. `prefix` is what the line reader already holds; `rest` returns
        // whatever arrived after the newline. A bad payload is still read to its newline before
        // the error, so the wire stays on a line boundary.
        public static byte[] ReadPayload(IntPtr h, string prefix, int want, int idleMs, out string rest) {
            long start = Stopwatch.GetTimestamp();
            long decode = 0;
            BvDropPayload payload = new BvDropPayload(want);
            byte[] held = Encoding.ASCII.GetBytes(prefix);
            int used = payload.Take(held, 0, held.Length);
            StringBuilder extra = new StringBuilder();
            if (used < prefix.Length) { extra.Append(prefix, used, prefix.Length - used); }
            long idleSince = 0;
            while (!payload.Done) {
                uint read;
                if (!ReadFile(h, Scratch, (uint)Scratch.Length, out read, IntPtr.Zero) || read == 0) {
                    long now = Stopwatch.GetTimestamp();
                    if (idleSince == 0) { idleSince = now; }
                    long idle = now - idleSince;
                    if (idle > idleMs * Stopwatch.Frequency / 1000) { throw new TimeoutException("drop-payload-timeout"); }
                    // Spin for the first few ms: a 1 ms sleep is a 15.6 ms timer tick on Windows.
                    if (idle < Stopwatch.Frequency / 200) { Thread.Sleep(0); } else { Thread.Sleep(1); }
                    continue;
                }
                idleSince = 0;
                long t = Stopwatch.GetTimestamp();
                int take = payload.Take(Scratch, 0, (int)read);
                decode += Stopwatch.GetTimestamp() - t;
                if (take < (int)read) { extra.Append(Encoding.ASCII.GetString(Scratch, take, (int)read - take)); }
            }
            rest = extra.ToString();
            byte[] data = payload.Result();
            DecodeTicks += decode;
            ReadTicks += Stopwatch.GetTimestamp() - start - decode;
            return data;
        }
    }

    // One base64 payload line of `want` characters and its newline, decoded as it arrives, with
    // Convert.FromBase64String's rules: the base64 alphabet only, '=' only as the last one or two
    // characters, and a length that is a multiple of 4.
    public sealed class BvDropPayload {
        static readonly int[] Sextets = BuildSextets();
        readonly int want;
        readonly byte[] data;
        int have, outPos, acc, quad, pad;
        string error;

        static int[] BuildSextets() {
            int[] map = new int[256];
            for (int i = 0; i < 256; i++) { map[i] = -1; }
            string alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            for (int i = 0; i < 64; i++) { map[alphabet[i]] = i; }
            return map;
        }

        public BvDropPayload(int want) {
            this.want = want;
            data = new byte[want / 4 * 3];
            if (want % 4 != 0) { error = "drop-payload-length"; }
        }

        public bool Done { get { return have == want + 1; } }

        // Takes bytes up to the end of the line; returns how many it took.
        public int Take(byte[] src, int offset, int count) {
            int take = Math.Min(count, want + 1 - have);
            int i = 0;
            while (i < take) {
                // Whole quads ahead of the last one, the only one that can hold padding.
                if (quad == 0 && error == null) {
                    int quads = Math.Min(take - i, want - 4 - have) / 4;
                    for (; quads > 0; quads--) {
                        int p = offset + i;
                        int s0 = Sextets[src[p]], s1 = Sextets[src[p + 1]], s2 = Sextets[src[p + 2]], s3 = Sextets[src[p + 3]];
                        if ((s0 | s1 | s2 | s3) < 0) { break; }
                        int v = (s0 << 18) | (s1 << 12) | (s2 << 6) | s3;
                        data[outPos] = (byte)(v >> 16);
                        data[outPos + 1] = (byte)(v >> 8);
                        data[outPos + 2] = (byte)v;
                        outPos += 3;
                        i += 4;
                        have += 4;
                    }
                    if (i >= take) { break; }
                }
                One(src[offset + i]);
                i++;
            }
            return take;
        }

        void One(int c) {
            int at = have++;
            if (at == want) {
                if (c != '\n' && error == null) { error = "drop-payload-frame"; }
                return;
            }
            if (error != null) { return; }
            int v = Sextets[c];
            if (v < 0) {
                if (c != '=' || at < want - 2) { error = "drop-payload-char"; return; }
                pad++;
                v = 0;
            } else if (pad > 0) {
                error = "drop-payload-char";
                return;
            }
            acc = (acc << 6) | v;
            if (++quad == 4) {
                data[outPos] = (byte)(acc >> 16);
                data[outPos + 1] = (byte)(acc >> 8);
                data[outPos + 2] = (byte)acc;
                outPos += 3;
                quad = 0;
                acc = 0;
            }
        }

        public byte[] Result() {
            if (!Done) { throw new InvalidOperationException("drop-payload-incomplete"); }
            if (error != null) { throw new FormatException(error); }
            if (pad == 0) { return data; }
            byte[] exact = new byte[data.Length - pad];
            Buffer.BlockCopy(data, 0, exact, 0, exact.Length);
            return exact;
        }
    }
}
'@

function Test-DropReader {
    if ($null -eq $script:BvDropReader) {
        try {
            Add-Type -TypeDefinition $script:BvDropReaderSource -ErrorAction Stop
            $script:BvDropReader = $true
        } catch {
            $script:BvDropReader = $false
            Log ('DROP reader unavailable, using Read-Line: ' + $_.Exception.Message)
        }
    }
    return $script:BvDropReader
}

# Chunk replies skip Write-Bytes' per-write log line; a short write still goes through it.
function Write-DropLine($handle, [string]$line) {
    $data = [System.Text.Encoding]::ASCII.GetBytes($line + "`n")
    $written = [uint32]0
    $ok = $K::WriteFile($handle, $data, [uint32]$data.Length, [ref]$written, [IntPtr]::Zero)
    if (-not $ok -or $written -lt $data.Length) {
        $suffix = New-Object byte[] ($data.Length - [int]$written)
        [System.Array]::Copy($data, [int]$written, $suffix, 0, $suffix.Length)
        Write-Bytes $handle $suffix 'DROP'
    }
}

# One path component the host already cleaned for Windows. Anything else is refused, not
# repaired: no separators, invalid or reserved names, '.', '..', or trailing dots and spaces.
function Test-DropName([string]$Name) {
    if ([string]::IsNullOrEmpty($Name) -or $Name -eq '.' -or $Name -eq '..') { return $false }
    if ($Name.IndexOfAny([char[]]'<>:"/\|?*') -ge 0 -or $Name -match '[\x00-\x1F]') { return $false }
    if ($Name.EndsWith('.') -or $Name.EndsWith(' ')) { return $false }
    $stem = $Name.Split('.')[0].TrimEnd(' ')
    return -not ($stem -match '^(CON|PRN|AUX|NUL|COM[0-9\u00B9\u00B2\u00B3]|LPT[0-9\u00B9\u00B2\u00B3])$')
}

# The full path for a dropped item below the Desktop. Every parent must be a real folder, never a
# reparse point; an existing name becomes "name (2).ext" ("name (2)" for a folder). The path is
# returned as is; every file system call on it goes through Get-IoPath, so a dropped folder deeper
# than 260 characters lands too.
# Each dropped file is written to a staging folder and moved to its Desktop name at DROPEND, so a
# drop the agent never finished (a guest restart, the VM stopping) leaves no cut-short file under a
# real name, and a OneDrive Desktop never uploads one. The move must be a rename, so the folder is on
# the target's volume: the user's temp folder when it shares it, else a hidden folder on the Desktop
# itself (a relocated or redirected Desktop). Drops never cross a reparse point below the Desktop.
function Get-DropVolume([string]$Path) {
    if ($Path.StartsWith('\\')) {
        $parts = $Path.Substring(2).Split([char]92)
        if ($parts.Length -ge 2) { return ('\\' + $parts[0] + '\' + $parts[1]).ToUpperInvariant() }
    }
    if ($Path.Length -ge 2) { return $Path.Substring(0, 2).ToUpperInvariant() }
    return $Path
}

function Select-DropStaging([string]$Temp, [string]$Desktop, [string]$Target) {
    if ((Get-DropVolume $Temp) -ceq (Get-DropVolume $Target)) { return [System.IO.Path]::Combine($Temp, 'BridgeVM-drop') }
    return [System.IO.Path]::Combine($Desktop, '.bridgevm-drop')
}

function Get-DropStagingDirs {
    return @([System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), 'BridgeVM-drop'), [System.IO.Path]::Combine((Get-DropRoot), '.bridgevm-drop'))
}

function Get-DropStaging([string]$Target) {
    $desktop = Get-DropRoot
    $dir = Select-DropStaging ([System.IO.Path]::GetTempPath()) $desktop $Target
    if (-not [System.IO.Directory]::Exists($dir)) {
        $info = [System.IO.Directory]::CreateDirectory($dir)
        if ($dir.StartsWith($desktop)) { $info.Attributes = $info.Attributes -bor [System.IO.FileAttributes]::Hidden }
    }
    return $dir
}

# A drop job starts with no file open: anything left here is from an agent that stopped mid-drop.
function Clear-DropStaging {
    foreach ($dir in (Get-DropStagingDirs)) {
        if (-not [System.IO.Directory]::Exists($dir)) { continue }
        foreach ($file in [System.IO.Directory]::GetFiles($dir)) {
            if ([System.IO.Path]::GetFileName($file) -cmatch '^[0-9a-f]{32}\.part$') { try { [System.IO.File]::Delete($file) } catch { } }
        }
    }
}

# $Leaf in $Dir, or "stem (2).ext" and on when that name is taken.
function Get-FreeDropPath([string]$Dir, [string]$Leaf, [bool]$IsDir) {
    $stem = $Leaf
    $ext = ''
    if (-not $IsDir) {
        $stem = [System.IO.Path]::GetFileNameWithoutExtension($Leaf)
        $ext = [System.IO.Path]::GetExtension($Leaf)
    }
    $target = [System.IO.Path]::Combine($Dir, $Leaf)
    $n = 2
    while ([System.IO.File]::Exists((Get-IoPath $target)) -or [System.IO.Directory]::Exists((Get-IoPath $target))) {
        $target = [System.IO.Path]::Combine($Dir, ('{0} ({1}){2}' -f $stem, $n, $ext))
        $n++
    }
    return $target
}

function Get-DropRoot {
    $dir = [Environment]::GetFolderPath('Desktop')
    if ([string]::IsNullOrEmpty($dir) -or -not [System.IO.Directory]::Exists($dir)) { throw 'no-desktop' }
    return $dir
}

function Resolve-DropTarget([string]$Root, [string]$Rel, [bool]$IsDir) {
    $dir = $Root
    $parts = $Rel.Split([char]92)
    foreach ($part in $parts) { if (-not (Test-DropName $part)) { throw 'bad-name' } }
    for ($i = 0; $i -lt $parts.Length - 1; $i++) {
        $dir = [System.IO.Path]::Combine($dir, $parts[$i])
        $attr = [System.IO.File]::GetAttributes((Get-IoPath $dir))
        if (($attr -band [System.IO.FileAttributes]::Directory) -eq 0 -or ($attr -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'bad-parent' }
    }
    return Get-FreeDropPath $dir $parts[$parts.Length - 1] $IsDir
}

# >>> clipboard watch
# CLIPCAPS PUSH: a background thread checks the clipboard sequence number every
# 100 ms. On a change it cancels the main thread's ReadFile on the port, and
# Read-Line writes CLIPCHANGED, so the main thread stays the port's only writer
# and the notice never lands inside a reply. Only that ReadFile can be cancelled:
# inRead covers nothing else, and the lock keeps a cancel from reaching later
# I/O. vioser marks a waiting read cancelable, and a cancelled read takes no
# bytes. Compiled on the first CLIPCAPS PUSH, so boot does not pay for it.
$script:BvClipPush = $false
$script:BvClipWatchReady = $null
$script:BvClipWatchSource = @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
namespace BridgeVM {
    public static class BvClipWatch {
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool ReadFile(IntPtr h, byte[] buf, uint toRead, out uint read, IntPtr overlapped);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool CancelSynchronousIo(IntPtr thread);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern IntPtr OpenThread(uint access, bool inherit, uint id);
        [DllImport("kernel32.dll")]
        static extern uint GetCurrentThreadId();
        [DllImport("kernel32.dll")]
        static extern bool CloseHandle(IntPtr h);
        [DllImport("user32.dll")]
        static extern uint GetClipboardSequenceNumber();
        const uint ThreadTerminate = 0x0001;
        const int OperationAborted = 995;
        static readonly object Gate = new object();
        static IntPtr reader = IntPtr.Zero;
        static uint readerId;
        static bool inRead;
        static bool pending;
        static Thread watcher;
        public static long Notices;

        public static void Start() {
            lock (Gate) {
                if (watcher != null) { return; }
                watcher = new Thread(Watch);
                watcher.IsBackground = true;
                watcher.Name = "BvClipWatch";
                watcher.Start();
            }
        }

        static void Watch() {
            uint seen = GetClipboardSequenceNumber();
            while (true) {
                Thread.Sleep(100);
                // An exception here would end the agent process, not just the watcher.
                try {
                    uint seq = GetClipboardSequenceNumber();
                    lock (Gate) {
                        if (seq != seen) { seen = seq; pending = true; }
                        if (pending && inRead) { CancelSynchronousIo(reader); }
                    }
                } catch (Exception) { }
            }
        }

        // Bytes read, 0 when the read failed, or -1 when it was cancelled for a
        // clipboard change.
        public static int Read(IntPtr h, byte[] buf) {
            lock (Gate) {
                uint id = GetCurrentThreadId();
                if (id != readerId || reader == IntPtr.Zero) {
                    if (reader != IntPtr.Zero) { CloseHandle(reader); }
                    reader = OpenThread(ThreadTerminate, false, id);
                    readerId = id;
                }
                inRead = true;
            }
            uint read;
            bool ok = ReadFile(h, buf, (uint)buf.Length, out read, IntPtr.Zero);
            int error = ok ? 0 : Marshal.GetLastWin32Error();
            lock (Gate) {
                inRead = false;
                if (!ok && error == OperationAborted && pending) {
                    pending = false;
                    Notices++;
                    return -1;
                }
            }
            return ok ? (int)read : 0;
        }
    }
}
'@
function Initialize-ClipWatch {
    if ($null -eq $script:BvClipWatchReady) {
        try {
            Add-Type -TypeDefinition $script:BvClipWatchSource -ErrorAction Stop
            [BridgeVM.BvClipWatch]::Start()
            $script:BvClipWatchReady = $true
        } catch {
            Log ('CLIP WATCH unavailable: ' + $_.Exception.Message)
            $script:BvClipWatchReady = $false
        }
    }
    return $script:BvClipWatchReady
}
# <<< clipboard watch

# >>> share watch
# The host lists the shared folder (LSR) every 2 s. Enumerating it costs the guest about
# 35 us per file each time (8,000 files: 111 ms/s while idle), so LSR re-sends its last
# listing while this watcher saw no change below the root. Events arrive on pool threads;
# the counter is read before each listing, so a change made during one is listed next time.
$script:BvShareWatchSource = @'
using System;
using System.IO;
using System.Threading;
namespace BridgeVM {
    public static class BvShareWatch {
        static FileSystemWatcher watcher;
        static string root;
        static long changes;
        static volatile bool broken;

        // Watch `path` (anew when it is another root or the watcher failed); false when it cannot.
        public static bool Watch(string path) {
            if (watcher != null && !broken && string.Equals(root, path, StringComparison.OrdinalIgnoreCase)) { return true; }
            if (watcher != null) { watcher.Dispose(); watcher = null; }
            Interlocked.Increment(ref changes);
            if (!Directory.Exists(path)) { return false; }
            FileSystemWatcher next = new FileSystemWatcher(path);
            next.IncludeSubdirectories = true;
            next.NotifyFilter = NotifyFilters.FileName | NotifyFilters.DirectoryName | NotifyFilters.Size | NotifyFilters.LastWrite;
            next.InternalBufferSize = 65536;
            next.Changed += Bump;
            next.Created += Bump;
            next.Deleted += Bump;
            next.Renamed += delegate (object sender, RenamedEventArgs e) { Interlocked.Increment(ref changes); };
            // A lost event (buffer overflow) or a dead watcher (root removed): list again, rewatch.
            next.Error += delegate (object sender, ErrorEventArgs e) { broken = true; Interlocked.Increment(ref changes); };
            broken = false;
            next.EnableRaisingEvents = true;
            watcher = next;
            root = path;
            return true;
        }

        static void Bump(object sender, FileSystemEventArgs e) { Interlocked.Increment(ref changes); }

        public static long Changes { get { return Interlocked.Read(ref changes); } }
    }
}
'@
$script:BvShareWatchReady = $null
$script:BvLsrRoot = $null
$script:BvLsrChanges = [long]-1
$script:BvLsrAt = [long]0
$script:BvLsrLine = $null
# Re-sent for at most this long without a fresh enumeration, whatever the watcher says.
$script:BvLsrMaxAgeTicks = [long](60 * [System.Diagnostics.Stopwatch]::Frequency)

# True while `root` is watched; the listing is then cached between changes.
function Watch-ShareRoot([string]$root) {
    if ($null -eq $script:BvShareWatchReady) {
        try {
            Add-Type -TypeDefinition $script:BvShareWatchSource -ErrorAction Stop
            $script:BvShareWatchReady = $true
        } catch {
            Log ('SHARE WATCH unavailable: ' + $_.Exception.Message)
            $script:BvShareWatchReady = $false
        }
    }
    if (-not $script:BvShareWatchReady) { return $false }
    try { return [BridgeVM.BvShareWatch]::Watch($root) } catch { Log ('SHARE WATCH failed: ' + $_.Exception.Message); return $false }
}
# <<< share watch

function Read-DataPayload($h, [int]$want) {
    # The payload line after a DROPCHUNK or PUTDATA header, <want> base64 characters: decoded as
    # it arrives (BvDropReader), else read as a line.
    if ($script:BvLines.Count -gt 0) { return , [System.Convert]::FromBase64String($script:BvLines.Dequeue()) }
    if (Test-DropReader) {
        $rest = ''
        $data = [BridgeVM.BvDropReader]::ReadPayload($h, $script:BvPending.ToString(), $want, 30000, [ref]$rest)
        $script:BvPending.Length = 0
        if ($rest.Contains("`n")) {
            $restParts = $rest.Split([char]10)
            for ($i = 0; $i -lt $restParts.Length - 1; $i++) { $script:BvLines.Enqueue($restParts[$i].TrimEnd([char]13)) }
            $rest = $restParts[$restParts.Length - 1]
        }
        [void]$script:BvPending.Append($rest)
        return , $data
    }
    $payload = $null
    for ($i = 0; $null -eq $payload -and $i -lt 15; $i++) { $payload = Read-Line $h }
    return , [System.Convert]::FromBase64String([string]$payload)
}

function Read-Line($handle) {
    # Full line, or $null after ~2s idle so the caller can poll port state.
    if ($script:BvLines.Count -gt 0) { return $script:BvLines.Dequeue() }
    $spins = 0
    while ($true) {
        if (Is-BadHandle $handle) { return $null }
        $read = 0
        if ($script:BvClipPush) {
            $read = [BridgeVM.BvClipWatch]::Read($handle, $script:BvReadBuffer)
            if ($read -lt 0) { Write-Line $handle 'CLIPCHANGED' 'CLIPCHANGED'; continue }
            $ok = $read -gt 0
        } else {
            $ok = $K::ReadFile($handle, $script:BvReadBuffer, [uint32]$script:BvReadBuffer.Length, [ref]$read, [IntPtr]::Zero)
        }
        if (-not $ok -or $read -eq 0) {
            if ($script:BvLines.Count -eq 0) {
                $spins++
                if ($spins -ge 133) { return $null }
            }
            Start-Sleep -Milliseconds 15
            continue
        }
        $spins = 0
        $chunk = [System.Text.Encoding]::ASCII.GetString($script:BvReadBuffer, 0, [int]$read)
        [void]$script:BvPending.Append($chunk)
        if ($chunk.Contains("`n")) {
            $text = $script:BvPending.ToString()
            $parts = $text.Split([char]10)
            $last = $parts.Length - 1
            for ($i = 0; $i -lt $last; $i++) {
                $line = $parts[$i]
                if ($line.EndsWith("`r")) { $line = $line.Substring(0, $line.Length - 1) }
                $script:BvLines.Enqueue($line)
            }
            $script:BvPending.Length = 0
            [void]$script:BvPending.Append($parts[$last])
            if ($script:BvLines.Count -gt 0) { return $script:BvLines.Dequeue() }
        }
    }
}
function Invoke-B64([string]$b64, [bool]$usePwsh) {
    $cmd = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($b64))
    if ($usePwsh) {
        $out = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $cmd 2>&1 | Out-String)
    } else {
        $out = (& cmd.exe /c $cmd 2>&1 | Out-String)
    }
    if ($null -eq $out) { $out = '' }
    return @{ Exit = $LASTEXITCODE; Out = $out }
}

function Write-CommandResult($handle, $result) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes([string]$result.Out)
    $chunk = 24576
    $max = 16 * 1024 * 1024
    if ($bytes.Length -gt $max) {
        $message = [System.Text.Encoding]::UTF8.GetBytes("command output exceeded ${max}-byte protocol limit")
        Write-Line $handle ("OUT -1 " + [System.Convert]::ToBase64String($message)) 'OUT-LIMIT'
        return
    }
    if ($bytes.Length -le $chunk) {
        Write-Line $handle ("OUT " + $result.Exit + " " + [System.Convert]::ToBase64String($bytes)) 'OUT'
        return
    }

    # Large shell output must not be one unbounded protocol line. The same
    # 24-KiB raw chunk size used by GET keeps every base64 frame near 32 KiB.
    $n = [int][System.Math]::Ceiling($bytes.Length / [double]$chunk)
    Write-Line $handle ("OUTBEG " + $result.Exit + " " + $bytes.Length + " " + $n) 'OUTBEG'
    $seq = 0
    $off = 0
    while ($off -lt $bytes.Length) {
        $len = [System.Math]::Min($chunk, $bytes.Length - $off)
        $slice = New-Object byte[] $len
        [System.Array]::Copy($bytes, $off, $slice, 0, $len)
        Write-Line $handle ("OUTCHUNK " + $seq + " " + [System.Convert]::ToBase64String($slice)) 'OUTCHUNK'
        $off += $len
        $seq++
    }
    Write-Line $handle ("OUTEND " + $seq) 'OUTEND'
}

# >>> clipboard images (CLIPCAPS, CLIPX, CLIPSETIMG)
# PNG bytes move as files in $script:BvClipDir through PUTBEG/PUTCHUNK/PUTEND
# and GET, never in a command line. BridgeVM.BvClipImage reads and sets the
# clipboard on its own STA thread: Windows Forms needs STA, and a wedged
# clipboard call must not hold this loop. It is compiled on the first CLIPCAPS;
# when that fails the agent answers a bare CLIPCAPS and the host keeps text only.
# CLIPSET writes the text from a short-lived STA thread. The thread that sets the
# clipboard becomes its owner, and a later copy in any guest app first waits for the
# owner to answer. This agent's thread sits in ReadFile without pumping messages, so
# each such copy stalled about 5 s (the hung-window timeout). A thread that has
# exited owns nothing. Each such thread ends with Application.ExitThread(): WinForms
# keeps a context with a duplicate of the thread's handle for every thread that touched
# the clipboard until the thread disposes it, so without it each call kept one handle
# (a 2-hour soak: 1,048 thread handles).
$script:BvClipTextSource = @'
using System;
using System.Threading;
using System.Windows.Forms;
namespace BridgeVM {
    public static class BvClipText {
        public static void Set(string text) {
            Exception failure = null;
            Thread thread = new Thread(delegate () {
                try {
                    if (string.IsNullOrEmpty(text)) { Clipboard.Clear(); }
                    else { Clipboard.SetDataObject(new DataObject(DataFormats.UnicodeText, text), true, 10, 100); }
                } catch (Exception e) { failure = e; } finally { Application.ExitThread(); }
            });
            thread.TrySetApartmentState(ApartmentState.STA);
            thread.IsBackground = true;
            thread.Start();
            if (!thread.Join(20000)) { throw new TimeoutException("the clipboard did not answer"); }
            if (failure != null) { throw new InvalidOperationException(failure.Message, failure); }
        }
    }
}
'@

# Without the helper (it failed to compile), Set-Clipboard as before.
function Set-ClipText([string]$text) {
    if ($null -eq $script:BvClipTextReady) {
        try {
            Add-Type -TypeDefinition $script:BvClipTextSource -ReferencedAssemblies System.Windows.Forms -ErrorAction Stop
            $script:BvClipTextReady = $true
        } catch {
            Log ('CLIP TEXT helper unavailable, using Set-Clipboard: ' + $_.Exception.Message)
            $script:BvClipTextReady = $false
        }
    }
    if ($script:BvClipTextReady) { [BridgeVM.BvClipText]::Set($text) } else { Set-Clipboard -Value $text -ErrorAction Stop }
}

$script:BvClipImageSource = @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;
namespace BridgeVM {
    public static class BvClipImage {
        public const int MaxBytes = 33554432;
        public const int MaxSide = 16384;
        public const int HtmlMaxBytes = 4194304;
        public const int FilesMax = 256;

        [DllImport("user32.dll", SetLastError = true)] static extern bool OpenClipboard(IntPtr owner);
        [DllImport("user32.dll")] static extern bool CloseClipboard();
        [DllImport("user32.dll")] static extern IntPtr GetClipboardData(uint format);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern uint RegisterClipboardFormat(string name);
        [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr memory);
        [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr memory);
        [DllImport("kernel32.dll")] static extern UIntPtr GlobalSize(IntPtr memory);

        static void OnSta(ThreadStart body) {
            Exception failure = null;
            Thread thread = new Thread(delegate () {
                // ExitThread releases the thread's WinForms context (see BvClipTextSource).
                try { body(); } catch (Exception e) { failure = e; } finally { Application.ExitThread(); }
            });
            // Always succeeds on Windows before Start; Clipboard refuses a non-STA thread anyway.
            thread.TrySetApartmentState(ApartmentState.STA);
            thread.IsBackground = true;
            thread.Start();
            if (!thread.Join(20000)) { throw new TimeoutException("the clipboard did not answer"); }
            if (failure != null) { throw new InvalidOperationException(failure.Message, failure); }
        }

        // Width and height from a PNG's signature and IHDR; false when it is no PNG.
        public static bool PngSize(byte[] png, out int width, out int height) {
            width = 0;
            height = 0;
            byte[] head = new byte[] { 137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82 };
            if (png == null || png.Length < 33) { return false; }
            for (int i = 0; i < head.Length; i++) { if (png[i] != head[i]) { return false; } }
            long w = ((long)png[16] << 24) | ((long)png[17] << 16) | ((long)png[18] << 8) | png[19];
            long h = ((long)png[20] << 24) | ((long)png[21] << 16) | ((long)png[22] << 8) | png[23];
            if (w < 1 || h < 1 || w > int.MaxValue || h > int.MaxValue) { return false; }
            width = (int)w;
            height = (int)h;
            return true;
        }

        public static string Sha256Hex(byte[] bytes) {
            using (SHA256 sha = SHA256.Create()) {
                StringBuilder hex = new StringBuilder(64);
                foreach (byte b in sha.ComputeHash(bytes)) { hex.Append(b.ToString("x2")); }
                return hex.ToString();
            }
        }

        // One clipboard write of the text (when not empty) and the PNG: a bitmap for
        // Paint and Word, and the PNG bytes as the registered "PNG" format, which keeps
        // transparency and reads back unchanged. Returns "<width> <height>".
        public static string Set(byte[] png, string text) {
            int width, height;
            if (png == null || png.Length > MaxBytes || !PngSize(png, out width, out height) || width > MaxSide || height > MaxSide) {
                throw new InvalidDataException("not a PNG within the clipboard image limits");
            }
            OnSta(delegate () {
                using (MemoryStream source = new MemoryStream(png))
                using (Bitmap bitmap = new Bitmap(source)) {
                    DataObject data = new DataObject();
                    if (!string.IsNullOrEmpty(text)) { data.SetData(DataFormats.UnicodeText, true, text); }
                    data.SetData(DataFormats.Bitmap, true, bitmap);
                    data.SetData("PNG", false, new MemoryStream(png));
                    Clipboard.SetDataObject(data, true, 10, 100);
                }
            });
            return width + " " + height;
        }

        // The clipboard image as PNG: the "PNG" format's own bytes when they are a PNG,
        // else the bitmap encoded as PNG. Null without an image; an image over MaxSide
        // returns null with its size set.
        public static byte[] Get(out int width, out int height) {
            byte[] result = null;
            int w = 0, h = 0;
            OnSta(delegate () {
                IDataObject data = Clipboard.GetDataObject();
                if (data == null) { return; }
                if (data.GetDataPresent("PNG", false)) {
                    MemoryStream stream = data.GetData("PNG", false) as MemoryStream;
                    int pw, ph;
                    if (stream != null) {
                        byte[] bytes = stream.ToArray();
                        if (PngSize(bytes, out pw, out ph)) { result = bytes; w = pw; h = ph; return; }
                    }
                }
                if (!data.GetDataPresent(DataFormats.Bitmap, true)) { return; }
                using (Image image = data.GetData(DataFormats.Bitmap, true) as Image) {
                    if (image == null) { return; }
                    w = image.Width;
                    h = image.Height;
                    if (w > MaxSide || h > MaxSide) { return; }
                    using (MemoryStream output = new MemoryStream()) {
                        image.Save(output, ImageFormat.Png);
                        result = output.ToArray();
                    }
                }
            });
            width = w;
            height = h;
            return result;
        }

        // One clipboard write of the text (when not empty) and the exact "HTML Format"
        // bytes, NUL-terminated as browsers write them. Windows Forms' own HTML path
        // would encode the string as ANSI; the format is UTF-8.
        public static void SetHtml(string text, byte[] html) {
            if (html == null || html.Length == 0 || html.Length > HtmlMaxBytes) {
                throw new InvalidDataException("no HTML within the clipboard limit");
            }
            byte[] terminated = new byte[html.Length + 1];
            Buffer.BlockCopy(html, 0, terminated, 0, html.Length);
            OnSta(delegate () {
                DataObject data = new DataObject();
                if (!string.IsNullOrEmpty(text)) { data.SetData(DataFormats.UnicodeText, true, text); }
                data.SetData("HTML Format", false, new MemoryStream(terminated));
                Clipboard.SetDataObject(data, true, 10, 100);
            });
        }

        // The clipboard's "HTML Format" bytes up to the first NUL, read raw (Windows
        // Forms would decode them as ANSI). Null without HTML, or over HtmlMaxBytes
        // with size set.
        public static byte[] GetHtml(out int size) {
            byte[] result = null;
            int n = 0;
            OnSta(delegate () {
                uint format = RegisterClipboardFormat("HTML Format");
                bool open = false;
                for (int i = 0; i < 10 && !(open = OpenClipboard(IntPtr.Zero)); i++) { Thread.Sleep(20); }
                if (!open) { throw new IOException("the clipboard is busy"); }
                try {
                    IntPtr memory = GetClipboardData(format);
                    if (memory == IntPtr.Zero) { return; }
                    IntPtr p = GlobalLock(memory);
                    if (p == IntPtr.Zero) { return; }
                    try {
                        long total = (long)GlobalSize(memory).ToUInt64();
                        byte[] bytes = new byte[(int)Math.Min(total, (long)HtmlMaxBytes + 1)];
                        Marshal.Copy(p, bytes, 0, bytes.Length);
                        int end = Array.IndexOf(bytes, (byte)0);
                        if (end < 0) { end = bytes.Length; }
                        if (end > HtmlMaxBytes) { n = (int)Math.Min(total, int.MaxValue); return; }
                        if (end == 0) { return; }
                        n = end;
                        result = new byte[end];
                        Buffer.BlockCopy(bytes, 0, result, 0, end);
                    } finally { GlobalUnlock(memory); }
                } finally { CloseClipboard(); }
            });
            size = n;
            return result;
        }

        // The clipboard's file list (CF_HDROP, an Explorer copy) as full paths; null without one.
        public static string[] GetFiles() {
            string[] result = null;
            OnSta(delegate () {
                if (!Clipboard.ContainsFileDropList()) { return; }
                System.Collections.Specialized.StringCollection list = Clipboard.GetFileDropList();
                result = new string[list.Count];
                list.CopyTo(result, 0);
            });
            return result;
        }

        // One clipboard write of an Explorer copy: the paths as a file drop, Preferred
        // DropEffect copy (1) so a paste copies, and the text when not empty.
        public static void SetFiles(string[] paths, string text) {
            OnSta(delegate () {
                DataObject data = new DataObject();
                data.SetData(DataFormats.FileDrop, true, paths);
                data.SetData("Preferred DropEffect", false, new MemoryStream(BitConverter.GetBytes(1)));
                if (!string.IsNullOrEmpty(text)) { data.SetData(DataFormats.UnicodeText, true, text); }
                Clipboard.SetDataObject(data, true, 10, 100);
            });
        }
    }
}
'@
$script:BvClipDir = Join-Path ([System.IO.Path]::GetTempPath()) 'BridgeVM-clip'
$script:BvClipImageReady = $null
# Like BvClipSeq for CLIPGET: the image part is read again only after a change.
$script:BvClipImgSeq = [uint32]4294967295
$script:BvClipImgDesc = '0 0 0 - -'

function Initialize-ClipImage {
    if ($null -eq $script:BvClipImageReady) {
        try {
            Add-Type -TypeDefinition $script:BvClipImageSource -ReferencedAssemblies System.Windows.Forms, System.Drawing -ErrorAction Stop
            [void][System.IO.Directory]::CreateDirectory($script:BvClipDir)
            $script:BvClipImageReady = $true
        } catch {
            Log ('CLIP IMAGE unavailable: ' + $_.Exception.Message)
            $script:BvClipImageReady = $false
        }
    }
    return $script:BvClipImageReady
}

# The image part of CLIPX: <w> <h> <bytes> <sha256|-> <b64(path)|->. A readable
# image within the limits is written to guest.png for the host's GET; the previous
# copy's file goes first. A failed read counts as no image for that change.
function Get-ClipImageDescriptor {
    $seq = $K::GetClipboardSequenceNumber()
    if ($seq -eq $script:BvClipImgSeq) { return $script:BvClipImgDesc }
    $desc = '0 0 0 - -'
    if ($script:BvClipImageReady) {
        try {
            $guestPath = $script:BvClipDir + '\guest.png'
            [void][System.IO.Directory]::CreateDirectory($script:BvClipDir)
            [System.IO.File]::Delete($guestPath)
            $width = 0
            $height = 0
            $png = [BridgeVM.BvClipImage]::Get([ref]$width, [ref]$height)
            if ($null -eq $png) {
                $desc = '{0} {1} 0 - -' -f $width, $height
            } elseif ($png.Length -gt [BridgeVM.BvClipImage]::MaxBytes) {
                $desc = '{0} {1} {2} - -' -f $width, $height, $png.Length
            } else {
                [System.IO.File]::WriteAllBytes($guestPath, $png)
                $desc = '{0} {1} {2} {3} {4}' -f $width, $height, $png.Length, [BridgeVM.BvClipImage]::Sha256Hex($png), [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($guestPath))
            }
        } catch {
            Log ('CLIPX image read failed: ' + $_.Exception.Message)
            $desc = '0 0 0 - -'
        }
    }
    $script:BvClipImgDesc = $desc
    $script:BvClipImgSeq = $seq
    return $desc
}

$script:BvClipHtmlSeq = [uint32]4294967295
$script:BvClipHtmlPart = '0 -'
# The clipboard's "HTML Format" bytes (Bytes, null without) and their size.
function Read-ClipHtml {
    $size = 0
    $bytes = [BridgeVM.BvClipImage]::GetHtml([ref]$size)
    return @{ Bytes = $bytes; Size = $size }
}
# The HTML part of CLIPXH: <bytes> <b64(cf_html)|->, read again only after a change.
# Over the limit it names the size only; a failed read counts as no HTML.
function Get-ClipHtmlPart {
    $seq = $K::GetClipboardSequenceNumber()
    if ($seq -eq $script:BvClipHtmlSeq) { return $script:BvClipHtmlPart }
    $part = '0 -'
    if ($script:BvClipImageReady) {
        try {
            $html = Read-ClipHtml
            if ($null -ne $html.Bytes) { $part = '{0} {1}' -f $html.Bytes.Length, [System.Convert]::ToBase64String($html.Bytes) }
            elseif ($html.Size -gt 0) { $part = '{0} -' -f $html.Size }
        } catch {
            Log ('CLIPXH html read failed: ' + $_.Exception.Message)
        }
    }
    $script:BvClipHtmlPart = $part
    $script:BvClipHtmlSeq = $seq
    return $part
}

$script:BvClipFilesSeq = [uint32]4294967295
$script:BvClipFilesPart = '0 -'
# The files part of CLIPXF: <count> <b64(list)|->, read again only after a change. The list has one
# line per entry, <isDir>|<top>|<bytes>|<mtime>|<full path>: each copied item that still exists (top
# 1), and below a copied folder every file and folder in it (top 0, after their folder). A folder has
# 0 bytes and mtime 0; a file's mtime (LastWriteTimeUtc ticks) tells the host an edited file from
# the copy it already has. Past FilesMax entries the listing stops and names the count only (FilesMax + 1). A failed
# read counts as no files.
function Get-ClipFilesPart {
    $seq = $K::GetClipboardSequenceNumber()
    if ($seq -eq $script:BvClipFilesSeq) { return $script:BvClipFilesPart }
    $part = '0 -'
    if ($script:BvClipImageReady) {
        try {
            $files = [BridgeVM.BvClipImage]::GetFiles()
            if ($null -ne $files) {
                $max = [BridgeVM.BvClipImage]::FilesMax
                $sb = New-Object System.Text.StringBuilder
                $count = 0
                $pending = New-Object System.Collections.Generic.Stack[object]
                for ($i = $files.Length - 1; $i -ge 0; $i--) { $pending.Push(@($files[$i], 1)) }
                while ($pending.Count -gt 0 -and $count -le $max) {
                    $entry = $pending.Pop()
                    $path = [string]$entry[0]
                    $io = Get-IoPath $path
                    if ([System.IO.Directory]::Exists($io)) {
                        [void]$sb.Append('1|').Append($entry[1]).Append('|0|0|').Append($path).Append("`n")
                        $children = @([System.IO.Directory]::GetFileSystemEntries($io))
                        [System.Array]::Sort($children, [System.StringComparer]::OrdinalIgnoreCase)
                        for ($j = $children.Length - 1; $j -ge 0; $j--) {
                            $pending.Push(@([System.IO.Path]::Combine($path, [System.IO.Path]::GetFileName($children[$j])), 0))
                        }
                    } elseif ([System.IO.File]::Exists($io)) {
                        $info = New-Object System.IO.FileInfo $io
                        [void]$sb.Append('0|').Append($entry[1]).Append('|').Append($info.Length).Append('|').Append($info.LastWriteTimeUtc.Ticks).Append('|').Append($path).Append("`n")
                    } else { continue }
                    $count++
                }
                if ($count -gt $max) { $part = '{0} -' -f $count }
                elseif ($count -gt 0) { $part = '{0} {1}' -f $count, [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($sb.ToString())) }
            }
        } catch {
            Log ('CLIPXF files read failed: ' + $_.Exception.Message)
        }
    }
    $script:BvClipFilesPart = $part
    $script:BvClipFilesSeq = $seq
    return $part
}

# A CLIPSETFILES path, relative to one copy's folder: every part is a name (not empty, . or ..)
# without a drive, stream or wildcard character or a control character, so it stays in that folder.
function Test-ClipRel([string]$rel) {
    if ([string]::IsNullOrEmpty($rel)) { return $false }
    foreach ($part in $rel.Split('\')) {
        if ($part -eq '' -or $part -eq '.' -or $part -eq '..' -or $part.IndexOfAny([char[]]':/*?"<>|') -ge 0) { return $false }
        foreach ($c in $part.ToCharArray()) { if ([int]$c -lt 32) { return $false } }
    }
    return $true
}
# <<< clipboard images

# One-shot sanity probe: does CreateFile work AT ALL in this process context, and
# what does the port's GetLastError actually mean? Opening \\.\C: (a known-good
# device) isolates a vioser-specific open failure from a general P/Invoke / token
# problem. FormatMessage turns the numeric gle (e.g. 203) into readable text.
function Win32Msg([int]$code) {
    try { return (New-Object System.ComponentModel.Win32Exception($code)).Message } catch { return "?" }
}
# Test a plain file first (ntdll.dll always exists) to prove the P/Invoke itself
# works with the [uint32] access constants; then \\.\C: for device access.
$probeF = $K::CreateFile('C:\Windows\System32\ntdll.dll', $GENERIC_READ, $FILE_SHARE_RW, [IntPtr]::Zero, $OPEN_EXISTING, 0, [IntPtr]::Zero)
$probeFGle = Gle
if (Is-BadHandle $probeF) {
    Log "SANITY open ntdll.dll FAILED gle=$probeFGle ($(Win32Msg $probeFGle)) -- CreateFile P/Invoke is broken"
} else {
    Log "SANITY open ntdll.dll OK -- CreateFile P/Invoke works"
    [void]$K::CloseHandle($probeF)
}
$probeC = $K::CreateFile('\\.\C:', $GENERIC_READ, $FILE_SHARE_RW, [IntPtr]::Zero, $OPEN_EXISTING, 0, [IntPtr]::Zero)
$probeGle = Gle
if (Is-BadHandle $probeC) {
    Log "SANITY open \\.\C: FAILED gle=$probeGle ($(Win32Msg $probeGle))"
} else {
    Log "SANITY open \\.\C: OK -- device CreateFile works; a port failure is vioser-specific"
    [void]$K::CloseHandle($probeC)
}

# Guest tools identity. The app installs and updates exactly these files and
# hashes them the same way (agent_console/agent_update.rs): SHA-256 over one
# "<name> <sha256 hex|missing>" line per file, in this order. READY carries the
# first 16 hex digits; the app updates an agent whose READY has none or another.
function Get-BvAgentId([string]$Dir) {
    $files = @('bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1', 'bvagent-update.ps1')
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $list = New-Object System.Text.StringBuilder
        foreach ($name in $files) {
            $path = [System.IO.Path]::Combine($Dir, $name)
            $digest = 'missing'
            if ([System.IO.File]::Exists($path)) {
                $digest = [System.BitConverter]::ToString($sha.ComputeHash([System.IO.File]::ReadAllBytes($path))).Replace('-', '').ToLowerInvariant()
            }
            [void]$list.Append($name + ' ' + $digest + "`n")
        }
        $all = $sha.ComputeHash([System.Text.Encoding]::ASCII.GetBytes($list.ToString()))
        return [System.BitConverter]::ToString($all, 0, 8).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}
# READY carries the ID, and update=pending while an update to this agent waits
# for the host's Confirm (bvagent-update.ps1 keeps update\pending.txt beside
# the version folders): the host then confirms it whatever it recorded itself.
function Get-BvReady([string]$Dir, [string]$Id) {
    $ready = 'READY ' + $env:COMPUTERNAME + ' v3-share2 putdata bigfile'
    if (-not $Id) { return $ready }
    $ready += ' agent=' + $Id
    $root = Split-Path -Parent $Dir
    if ($root) {
        $pending = [System.IO.Path]::Combine($root, 'update', 'pending.txt')
        try {
            if ([System.IO.File]::Exists($pending) -and ([System.IO.File]::ReadAllText($pending).Trim() -ceq $Id)) { $ready += ' update=pending' }
        } catch { }
    }
    return $ready
}
$bvAgentId = ''
try { $bvAgentId = Get-BvAgentId $PSScriptRoot } catch { Log ('AGENT ID unavailable: ' + $_.Exception.Message) }
Log ('AGENT ID ' + $bvAgentId)

# Open the port (retry forever; throwing would kill the process and release the
# singleton mutex, reviving churn). Try-OpenPort logs every candidate tried.
$h = [System.IntPtr]::Zero
$attempt = 0
while ($true) {
    $h = Try-OpenPort
    if (-not (Is-BadHandle $h)) { Log "PORT OPEN ok attempt=$attempt"; break }
    if (($attempt % 20) -eq 0) { Log "PORT OPEN waiting attempt=$attempt (no candidate opened)" }
    $attempt++
    Start-Sleep -Milliseconds 500
}

$bvReady = Get-BvReady $PSScriptRoot $bvAgentId
Write-Line $h $bvReady 'READY'
Log 'READY sent'
Log-PortInfo $h 'after-ready'

# Service loop: run forever. A single failure must not tear the port down, so
# the body is wrapped. On each ~2s idle, poll vioser's port state.
while ($true) {
    try {
        $line = Read-Line $h
        if ($null -eq $line) {
            Log-PortInfo $h 'idle'
        } elseif (-not [string]::IsNullOrWhiteSpace($line)) {
            $sp = $line.IndexOf(' ')
            $tok = if ($sp -lt 0) { $line } else { $line.Substring(0, $sp) }
            $arg = if ($sp -lt 0) { '' } else { $line.Substring($sp + 1) }
            switch ($tok) {
                'PING' { Write-Line $h 'PONG' 'PONG' }
                { $_ -in @('TEXTINPUT', 'KEYINPUT', 'POINTERINPUT') } { Write-CommandResult $h (Invoke-UnicodeInput $arg $null ($tok -eq 'KEYINPUT') ($tok -eq 'POINTERINPUT')) }
            'INPUTCAPS' { Write-CommandResult $h (Get-InputCapabilities $arg) }
                'WINLIST' {
                    # Materialize before writing: a provider failure must not
                    # publish a partial list. ERR alone is the failure terminal.
                    try {
                        $inventoryLines = @(Get-BvWindowInventoryLines)
                        foreach ($inventoryLine in $inventoryLines) {
                            Write-Line $h $inventoryLine 'WIN'
                        }
                        Write-Line $h 'WINEND' 'WINEND'
                    } catch {
                        Log ('WINLIST failed: ' + $_.Exception.Message)
                        Write-Line $h 'ERR WINLIST unavailable' 'ERR'
                    }
                }
                'WINBOUNDS' {
                    # WINBOUNDS <hwnd> <x> <y> <w> <h> -> SetWindowPos, no z change.
                    $p = $arg -split ' '
                    if ($p.Count -eq 5) {
                        $hw = [IntPtr][long]$p[0]
                        # SWP_NOZORDER(0x4) | SWP_NOACTIVATE(0x10)
                        if ($null -ne $W -and $W::SetWindowPos($hw, [IntPtr]::Zero, [int]$p[1], [int]$p[2], [int]$p[3], [int]$p[4], 0x14)) {
                            Write-Line $h 'OK WINBOUNDS' 'OK'
                        } else { Write-Line $h 'ERR WINBOUNDS' 'ERR' }
                    } else { Write-Line $h 'ERR WINBOUNDS args' 'ERR' }
                }
                'WINFOCUS' {
                    $hw = [IntPtr][long]$arg
                    if ($null -ne $W -and $W::SetForegroundWindow($hw)) { Write-Line $h 'OK WINFOCUS' 'OK' }
                    else { Write-Line $h 'ERR WINFOCUS' 'ERR' }
                }
                'WINCLOSE' {
                    # WM_CLOSE=0x10 posted, not sent: a hung window must not hang the agent.
                    $hw = [IntPtr][long]$arg
                    if ($null -ne $W -and $W::PostMessage($hw, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)) { Write-Line $h 'OK WINCLOSE' 'OK' }
                    else { Write-Line $h 'ERR WINCLOSE' 'ERR' }
                }
                'RUN'  { $r = Invoke-B64 $arg $false; Write-CommandResult $h $r }
                'PS'   { $r = Invoke-B64 $arg $true;  Write-CommandResult $h $r }
                'CLIPGET' {
                    # Return the guest Windows clipboard text as CLIP <base64(utf8)>.
                    # The host polls CLIPGET every second, and repeatedly opening
                    # the OLE clipboard from PS 5.1 wedges after a few hundred
                    # calls (live-observed: the agent froze inside Get-Clipboard
                    # ~5-6 min into every clipsync run, killing the channel). So
                    # gate the actual Get-Clipboard behind user32's
                    # GetClipboardSequenceNumber - a cheap, never-blocking query
                    # that bumps on every clipboard change - and serve a cached
                    # copy while the sequence is unchanged (idle polls open the
                    # clipboard zero times).
                    $seq = $K::GetClipboardSequenceNumber()
                    if ($seq -ne $script:BvClipSeq) {
                        $clip = ''
                        try { $c = Get-Clipboard -Raw -ErrorAction SilentlyContinue; if ($null -ne $c) { $clip = [string]$c } } catch { }
                        $script:BvClipCache = $clip
                        $script:BvClipSeq = $seq
                    }
                    Write-Line $h ("CLIP " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script:BvClipCache))) 'CLIP'
                }
                'CLIPX' {
                    # CLIPGET plus the clipboard image, for hosts that saw IMAGE_PNG:
                    # CLIPX <w> <h> <bytes> <sha256|-> <b64(path)|-> <b64(utf8 text)>.
                    $seq = $K::GetClipboardSequenceNumber()
                    if ($seq -ne $script:BvClipSeq) {
                        $clip = ''
                        try { $c = Get-Clipboard -Raw -ErrorAction SilentlyContinue; if ($null -ne $c) { $clip = [string]$c } } catch { }
                        $script:BvClipCache = $clip
                        $script:BvClipSeq = $seq
                    }
                    $image = Get-ClipImageDescriptor
                    Write-Line $h ('CLIPX ' + $image + ' ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script:BvClipCache))) 'CLIPX'
                }
                'CLIPXH' {
                    # CLIPX plus the clipboard's HTML, for hosts that saw CF_HTML: CLIPXH <w> <h>
                    # <bytes> <sha256|-> <b64(path)|-> <html-bytes> <b64(html)|-> <b64(utf8 text)>.
                    $seq = $K::GetClipboardSequenceNumber()
                    if ($seq -ne $script:BvClipSeq) {
                        $clip = ''
                        try { $c = Get-Clipboard -Raw -ErrorAction SilentlyContinue; if ($null -ne $c) { $clip = [string]$c } } catch { }
                        $script:BvClipCache = $clip
                        $script:BvClipSeq = $seq
                    }
                    $image = Get-ClipImageDescriptor
                    $html = Get-ClipHtmlPart
                    Write-Line $h ('CLIPXH ' + $image + ' ' + $html + ' ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script:BvClipCache))) 'CLIPXH'
                }
                'CLIPXF' {
                    # CLIPXH plus the clipboard's file list (an Explorer copy), for hosts that saw
                    # CF_HDROP: CLIPXF <w> <h> <bytes> <sha256|-> <b64(path)|-> <html-bytes>
                    # <b64(html)|-> <file-count> <b64(list)|-> <b64(utf8 text)>.
                    $seq = $K::GetClipboardSequenceNumber()
                    if ($seq -ne $script:BvClipSeq) {
                        $clip = ''
                        try { $c = Get-Clipboard -Raw -ErrorAction SilentlyContinue; if ($null -ne $c) { $clip = [string]$c } } catch { }
                        $script:BvClipCache = $clip
                        $script:BvClipSeq = $seq
                    }
                    $image = Get-ClipImageDescriptor
                    $html = Get-ClipHtmlPart
                    $files = Get-ClipFilesPart
                    Write-Line $h ('CLIPXF ' + $image + ' ' + $html + ' ' + $files + ' ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script:BvClipCache))) 'CLIPXF'
                }
                'CLIPCAPS' {
                    # CLIPCAPS [PUSH] -> CLIPCAPS [IMAGE_PNG <b64(dir)> CF_HTML CF_HDROP CLIPSETFILES]
                    # [CLIPPUSH]: IMAGE_PNG when this agent can move clipboard images, CF_HTML when it
                    # serves CLIPXH and CLIPSETH, CF_HDROP when it serves CLIPXF and CLIPSETFILES
                    # when it serves that verb (all need the same helper); CLIPPUSH when the host asked
                    # for PUSH and the watcher runs, so Read-Line writes CLIPCHANGED after each
                    # guest clipboard change. A bare CLIPCAPS (older host) turns notices off.
                    $script:BvClipPush = ($arg -ceq 'PUSH') -and (Initialize-ClipWatch)
                    $caps = 'CLIPCAPS'
                    if (Initialize-ClipImage) {
                        $caps += ' IMAGE_PNG ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script:BvClipDir)) + ' CF_HTML CF_HDROP CLIPSETFILES'
                    }
                    if ($script:BvClipPush) { $caps += ' CLIPPUSH' }
                    Write-Line $h $caps 'CLIPCAPS'
                }
                'CLIPSETIMG' {
                    # CLIPSETIMG <b64(path)> <b64(utf8 text)|-> -> OK CLIPSETIMG <w> <h>: one
                    # clipboard write of the text and the PNG file the host just PUT. The file
                    # becomes guest.png, which CLIPX names until the clipboard changes.
                    try {
                        if (-not (Initialize-ClipImage)) { throw 'clipboard images unavailable' }
                        $parts = $arg.Split(' ')
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))
                        $txt = ''
                        if ($parts.Length -ge 2 -and $parts[1] -ne '-') { $txt = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[1])) }
                        $png = [System.IO.File]::ReadAllBytes($path)
                        $size = [BridgeVM.BvClipImage]::Set($png, $txt)
                        $guestPath = $script:BvClipDir + '\guest.png'
                        [System.IO.File]::Delete($guestPath)
                        [System.IO.File]::Move($path, $guestPath)
                        $seq = $K::GetClipboardSequenceNumber()
                        $script:BvClipCache = $txt
                        $script:BvClipSeq = $seq
                        $script:BvClipImgDesc = '{0} {1} {2} {3}' -f $size, $png.Length, [BridgeVM.BvClipImage]::Sha256Hex($png), [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($guestPath))
                        $script:BvClipImgSeq = $seq
                        Write-Line $h ('OK CLIPSETIMG ' + $size) 'OK'
                    } catch { Write-Line $h ('ERR CLIPSETIMG ' + $_.Exception.Message) 'ERR' }
                }
                'CLIPSETH' {
                    # CLIPSETH <b64(utf8 text)|-> <b64(html)> -> OK CLIPSETH: one clipboard write of
                    # the text and the exact "HTML Format" bytes, which CLIPXH then serves as sent.
                    try {
                        if (-not (Initialize-ClipImage)) { throw 'clipboard html unavailable' }
                        $parts = $arg.Split(' ')
                        $txt = ''
                        if ($parts[0] -ne '-') { $txt = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0])) }
                        $html = [System.Convert]::FromBase64String($parts[1])
                        [BridgeVM.BvClipImage]::SetHtml($txt, $html)
                        $seq = $K::GetClipboardSequenceNumber()
                        $script:BvClipCache = $txt
                        $script:BvClipSeq = $seq
                        $script:BvClipImgDesc = '0 0 0 - -'
                        $script:BvClipImgSeq = $seq
                        $script:BvClipHtmlPart = '{0} {1}' -f $html.Length, $parts[1]
                        $script:BvClipHtmlSeq = $seq
                        Write-Line $h 'OK CLIPSETH' 'OK'
                    } catch { Write-Line $h ('ERR CLIPSETH ' + $_.Exception.Message) 'ERR' }
                }
                'CLIPSETFILES' {
                    # CLIPSETFILES <copy> <b64(list)> <b64(utf8 text)|-> -> OK CLIPSETFILES <n>: a Mac
                    # file copy the host just PUT below <clipdir>\files\<copy> becomes an Explorer
                    # copy. The list has one line per entry, <isDir>|<path in that folder>; a path
                    # that would leave it is refused, listed folders are created (an empty one too)
                    # and listed files must be there. The n top-level items go on the clipboard
                    # with the text in one write. Older copies' folders stay for a paste that may
                    # still read them: the 3 put on the clipboard last; older ones are deleted.
                    try {
                        if (-not (Initialize-ClipImage)) { throw 'clipboard files unavailable' }
                        $parts = $arg.Split(' ')
                        if ($parts.Length -lt 2 -or $parts[0] -notmatch '^[0-9]{1,19}$') { throw 'bad copy' }
                        $base = $script:BvClipDir + '\files'
                        $root = $base + '\' + $parts[0]
                        $tops = New-Object System.Collections.Generic.List[string]
                        foreach ($line in [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[1])).Split("`n")) {
                            if ($line -eq '') { continue }
                            $rel = $line.Substring([Math]::Min(2, $line.Length))
                            if ($line.Length -lt 3 -or $line[1] -ne '|' -or -not (Test-ClipRel $rel)) { throw ('unsafe path ' + $rel) }
                            $full = $root + '\' + $rel
                            $io = Get-IoPath $full
                            if ($line[0] -eq '1') { [void][System.IO.Directory]::CreateDirectory($io) }
                            elseif ($line[0] -ne '0' -or -not [System.IO.File]::Exists($io)) { throw ('missing ' + $rel) }
                            if ($rel.IndexOf('\') -lt 0) { $tops.Add($full) }
                        }
                        if ($tops.Count -eq 0) { throw 'no files' }
                        $txt = ''
                        if ($parts.Length -ge 3 -and $parts[2] -ne '-') { $txt = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[2])) }
                        [BridgeVM.BvClipImage]::SetFiles($tops.ToArray(), $txt)
                        try {
                            [System.IO.Directory]::SetLastWriteTimeUtc((Get-IoPath $root $true), [DateTime]::UtcNow)
                            $olds = @([System.IO.Directory]::GetDirectories($base) | Where-Object { [System.IO.Path]::GetFileName($_) -ne $parts[0] } |
                                Sort-Object { [System.IO.Directory]::GetLastWriteTimeUtc($_) } -Descending)
                            for ($i = 3; $i -lt $olds.Count; $i++) { [System.IO.Directory]::Delete((Get-IoPath $olds[$i] $true), $true) }
                        } catch { Log ('CLIPSETFILES cleanup failed: ' + $_.Exception.Message) }
                        $seq = $K::GetClipboardSequenceNumber()
                        $script:BvClipCache = $txt
                        $script:BvClipSeq = $seq
                        $script:BvClipImgDesc = '0 0 0 - -'
                        $script:BvClipImgSeq = $seq
                        $script:BvClipHtmlPart = '0 -'
                        $script:BvClipHtmlSeq = $seq
                        Write-Line $h ('OK CLIPSETFILES ' + $tops.Count) 'OK'
                    } catch { Write-Line $h ('ERR CLIPSETFILES ' + $_.Exception.Message) 'ERR' }
                }
                'CLIPSET' {
                    # Set the guest Windows clipboard from CLIPSET <base64(utf8)>.
                    # Refresh the CLIPGET cache to the text we just wrote so the
                    # following poll serves it without reopening the clipboard.
                    try {
                        $txt = if ([string]::IsNullOrEmpty($arg)) { '' } else { [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg)) }
                        Set-ClipText $txt
                        $script:BvClipCache = $txt
                        $script:BvClipSeq = $K::GetClipboardSequenceNumber()
                        Write-Line $h 'OK CLIPSET' 'OK'
                    } catch { Write-Line $h ("ERR CLIPSET " + $_.Exception.Message) 'ERR' }
                }
                'LS' {
                    # LS <b64(path)> -> LSOK <b64(listing)>; one entry per line as
                    # name|size|isDir(0/1)|mtimeUtc(ISO). Errors -> ERR LS <msg>.
                    try {
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg))
                        $sb = New-Object System.Text.StringBuilder
                        foreach ($e in Get-ChildItem -LiteralPath $path -Force -ErrorAction Stop) {
                            $sz = if ($e.PSIsContainer) { 0 } else { $e.Length }
                            [void]$sb.AppendLine(('{0}|{1}|{2}|{3}' -f $e.Name, $sz, [int]$e.PSIsContainer, $e.LastWriteTimeUtc.ToString('o')))
                        }
                        Write-Line $h ('LSOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($sb.ToString()))) 'LSOK'
                    } catch { Write-Line $h ('ERR LS ' + $_.Exception.Message) 'ERR' }
                }
                'LSR' {
                    # LSR <b64(root)> -> LSOK <b64(listing)>; recursive entries
                    # are relpath|size|isDir(0/1)|mtimeUtc(ISO), with relpath
                    # rooted below <root> and using backslash separators.
                    try {
                        $root = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg))
                        $rootFull = [System.IO.Path]::GetFullPath($root)
                        if (-not $rootFull.EndsWith('\')) { $rootFull = $rootFull + '\' }
                        # Unchanged since the last listing (share watch): the same reply again.
                        $watched = Watch-ShareRoot $rootFull
                        $changes = if ($watched) { [BridgeVM.BvShareWatch]::Changes } else { [long]-1 }
                        $now = [System.Diagnostics.Stopwatch]::GetTimestamp()
                        if ($watched -and $script:BvLsrRoot -eq $rootFull -and $script:BvLsrChanges -eq $changes -and ($now - $script:BvLsrAt) -lt $script:BvLsrMaxAgeTicks) {
                            $line = $script:BvLsrLine
                        } else {
                            $sb = New-Object System.Text.StringBuilder
                            # Listed below \\?\ so a path past 260 characters is listed, not an error
                            # that fails the whole listing.
                            $listRoot = Get-IoPath $rootFull $true
                            foreach ($e in Get-ChildItem -LiteralPath $listRoot -Recurse -Force -ErrorAction Stop) {
                                # The pattern is written out here: a variable that was ever unset would make
                                # -cmatch match every name and leave every file out of the listing.
                                if (-not $e.PSIsContainer -and $e.Name -cmatch '^\.bridgevm-part-[0-9a-f]{32}$') {
                                    # A large PUT's temporary (large put), never listed; an old one no PUT holds is a leftover.
                                    $held = $null -ne $script:BvLPut -and [string]::Equals([System.IO.Path]::GetFileName($script:BvLPut.Temp), $e.Name, [System.StringComparison]::OrdinalIgnoreCase)
                                    if (-not $held -and $e.LastWriteTimeUtc -lt [DateTime]::UtcNow.AddMinutes(-5)) { try { $e.Delete() } catch { } }
                                    continue
                                }
                                $full = $e.FullName
                                if (-not $full.StartsWith($listRoot, [System.StringComparison]::OrdinalIgnoreCase)) { continue }
                                $rel = $full.Substring($listRoot.Length)
                                if ([string]::IsNullOrEmpty($rel)) { continue }
                                $sz = if ($e.PSIsContainer) { 0 } else { $e.Length }
                                [void]$sb.AppendLine(('{0}|{1}|{2}|{3}' -f $rel, $sz, [int]$e.PSIsContainer, $e.LastWriteTimeUtc.ToString('o')))
                            }
                            $line = 'LSOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($sb.ToString()))
                            if ($watched) {
                                $script:BvLsrRoot = $rootFull
                                $script:BvLsrChanges = $changes
                                $script:BvLsrAt = $now
                                $script:BvLsrLine = $line
                            }
                        }
                        Write-Line $h $line 'LSOK'
                    } catch { Write-Line $h ('ERR LSR ' + $_.Exception.Message) 'ERR' }
                }
                'GET' {
                    # GET <b64(path)> -> GETBEG <b64(path)> <total> <nchunks>,
                    # then N x GETCHUNK <seq> <b64(rawbytes)>, then GETEND <seq>.
                    # Chunks are 24 KiB raw so each base64 line stays reasonable.
                    try {
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg))
                        $bytes = [System.IO.File]::ReadAllBytes((Get-IoPath $path))
                        $total = $bytes.Length
                        $chunk = 24576
                        $n = [int][System.Math]::Ceiling($total / [double]$chunk)
                        Write-Line $h ('GETBEG ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($path)) + ' ' + $total + ' ' + $n) 'GETBEG'
                        $seq = 0; $off = 0
                        while ($off -lt $total) {
                            $len = [System.Math]::Min($chunk, $total - $off)
                            $slice = New-Object byte[] $len
                            [System.Array]::Copy($bytes, $off, $slice, 0, $len)
                            Write-Line $h ('GETCHUNK ' + $seq + ' ' + [System.Convert]::ToBase64String($slice)) 'GETCHUNK'
                            $off += $len; $seq++
                        }
                        Write-Line $h ('GETEND ' + $seq) 'GETEND'
                    } catch { Write-Line $h ('ERR GET ' + $_.Exception.Message) 'ERR' }
                }
                'GETRANGE' {
                    # GETRANGE <b64(path)> <offset> <length> -> GETBEG <b64(path)> <bytes> <nchunks>
                    # <offset> <size> <mtime>, then GETCHUNK lines and GETEND as GET: up to <length>
                    # bytes from <offset>, with the file's size and time (as LSR lists it) read after
                    # those bytes, so a host that sees either change knows the file changed while it
                    # was copied. Chunks are 768 KiB raw (Write-Bytes sends 32 KiB writes).
                    try {
                        $p = $arg.Split(' ')
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($p[0]))
                        $offset = [int64]$p[1]
                        $want = [System.Math]::Min([int64]$p[2], [int64](16 * 1024 * 1024))
                        $io = Get-IoPath $path
                        $fs = [System.IO.File]::Open($io, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, ([System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete))
                        try {
                            $len = [int][System.Math]::Max([int64]0, [System.Math]::Min($want, $fs.Length - $offset))
                            $bytes = New-Object byte[] $len
                            $got = 0
                            if ($len -gt 0) {
                                [void]$fs.Seek($offset, [System.IO.SeekOrigin]::Begin)
                                while ($got -lt $len) {
                                    $n = $fs.Read($bytes, $got, $len - $got)
                                    if ($n -le 0) { break }
                                    $got += $n
                                }
                            }
                            $size = $fs.Length
                        } finally { $fs.Close() }
                        $mtime = [System.IO.File]::GetLastWriteTimeUtc($io).ToString('o')
                        $chunk = 786432
                        $n = [int][System.Math]::Ceiling($got / [double]$chunk)
                        Write-Line $h ('GETBEG ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($path)) + ' ' + $got + ' ' + $n + ' ' + $offset + ' ' + $size + ' ' + $mtime) 'GETBEG'
                        $seq = 0; $off = 0
                        while ($off -lt $got) {
                            $take = [System.Math]::Min($chunk, $got - $off)
                            Write-Line $h ('GETCHUNK ' + $seq + ' ' + [System.Convert]::ToBase64String($bytes, $off, $take)) 'GETCHUNK'
                            $off += $take; $seq++
                        }
                        Write-Line $h ('GETEND ' + $seq) 'GETEND'
                    } catch { Write-Line $h ('ERR GETRANGE ' + $_.Exception.Message) 'ERR' }
                }
                'PUT' {
                    # PUT <b64(path)> <b64(rawbytes)> [<mtime ms>] -> PUTOK <b64(path)> <bytes>.
                    # Single-chunk write; the host sends the whole file in one line.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        $parts = $arg.Split(' ')
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))
                        $data = if ($parts.Length -ge 2 -and $parts[1]) { [System.Convert]::FromBase64String($parts[1]) } else { New-Object byte[] 0 }
                        $io = Get-IoPath $path
                        $dir = [System.IO.Path]::GetDirectoryName($io)
                        if ($dir) { [void][System.IO.Directory]::CreateDirectory($dir) }
                        [System.IO.File]::WriteAllBytes($io, $data)
                        if ($parts.Length -ge 3) { Set-PutTime $path $parts[2] }
                        Write-Line $h ('PUTOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($path)) + ' ' + $data.Length) 'PUTOK'
                    } catch { Write-Line $h ('ERR PUT ' + $_.Exception.Message) 'ERR' }
                }
                'PUTBEG' {
                    # PUTBEG <b64(path)> <total> <nchunks> [<mtime ms>] -> OK PUTBEG.
                    # Opens/truncates a stream; PUTCHUNK/PUTEND complete it.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        Close-PutStream
                        $parts = $arg.Split(' ')
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))
                        $total = [int64]$parts[1]
                        $io = Get-IoPath $path
                        $dir = [System.IO.Path]::GetDirectoryName($io)
                        if ($dir) { [void][System.IO.Directory]::CreateDirectory($dir) }
                        $script:BvPutStream = [System.IO.File]::Open($io, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
                        $script:BvPutPath = $path
                        $script:BvPutTotal = $total
                        $script:BvPutWritten = [int64]0
                        $script:BvPutSeq = 0
                        if ($parts.Length -ge 4) { $script:BvPutTime = $parts[3] }
                        Write-Line $h 'OK PUTBEG' 'OK'
                    } catch {
                        Close-PutStream
                        Write-Line $h ('ERR PUTBEG ' + $_.Exception.Message) 'ERR'
                    }
                }
                'PUTCHUNK' {
                    # PUTCHUNK <seq> <b64(rawbytes)> -> OK PUTCHUNK <seq>.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        if ($null -eq $script:BvPutStream) { throw 'no-open-put' }
                        $sp2 = $arg.IndexOf(' ')
                        $seqText = if ($sp2 -lt 0) { $arg } else { $arg.Substring(0, $sp2) }
                        $payload = if ($sp2 -lt 0) { '' } else { $arg.Substring($sp2 + 1) }
                        $seq = [int]$seqText
                        if ($seq -ne $script:BvPutSeq) {
                            Write-Line $h 'ERR PUTCHUNK seq' 'ERR'
                        } else {
                            $data = if (-not [string]::IsNullOrEmpty($payload)) { [System.Convert]::FromBase64String($payload) } else { New-Object byte[] 0 }
                            $script:BvPutStream.Write($data, 0, $data.Length)
                            $script:BvPutWritten += [int64]$data.Length
                            $script:BvPutSeq++
                            Write-Line $h ('OK PUTCHUNK ' + $seq) 'OK'
                        }
                    } catch { Write-Line $h ('ERR PUTCHUNK ' + $_.Exception.Message) 'ERR' }
                }
                'PUTDATA' {
                    # PUTDATA <seq> <chars>, then the payload as its own line of <chars> base64
                    # characters -> OK PUTCHUNK <seq>, as PUTCHUNK. The payload is decoded as it
                    # arrives, as a drop chunk's: a PUTCHUNK line went through Read-Line and
                    # Convert.FromBase64String, about 140 ms per MiB. It is read before any check,
                    # so a refused chunk still leaves the wire on a line boundary.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        $p = $arg.Split(' ')
                        $seq = [int]$p[0]
                        $data = Read-DataPayload $h ([int]$p[1])
                        if ($null -eq $script:BvPutStream) { throw 'no-open-put' }
                        if ($seq -ne $script:BvPutSeq) {
                            Write-Line $h 'ERR PUTCHUNK seq' 'ERR'
                        } else {
                            $script:BvPutStream.Write($data, 0, $data.Length)
                            $script:BvPutWritten += [int64]$data.Length
                            $script:BvPutSeq++
                            Write-Line $h ('OK PUTCHUNK ' + $seq) 'OK'
                        }
                    } catch { Write-Line $h ('ERR PUTCHUNK ' + $_.Exception.Message) 'ERR' }
                }
                'PUTEND' {
                    # PUTEND <seq> -> PUTOK <b64(path)> <bytes> after byte-count
                    # verification. The terminal reply matches legacy PUT.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        if ($null -eq $script:BvPutStream) { throw 'no-open-put' }
                        $seq = [int]$arg
                        $path = $script:BvPutPath
                        $written = $script:BvPutWritten
                        $total = $script:BvPutTotal
                        $expectedSeq = $script:BvPutSeq
                        $time = $script:BvPutTime
                        Close-PutStream
                        if ($seq -ne $expectedSeq) {
                            Write-Line $h 'ERR PUTEND seq' 'ERR'
                        } elseif ($written -ne $total) {
                            Write-Line $h 'ERR PUTEND short' 'ERR'
                        } else {
                            Set-PutTime $path $time
                            Write-Line $h ('PUTOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($path)) + ' ' + $written) 'PUTOK'
                        }
                    } catch {
                        Close-PutStream
                        Write-Line $h ('ERR PUTEND ' + $_.Exception.Message) 'ERR'
                    }
                }
                'LPUTBEG' {
                    # LPUTBEG <b64(path)> <total> [<mtime ms>] -> OK LPUTBEG: a large file's own stream into a
                    # new temporary beside <path> (large put); ERR LPUTBEG disk-full when the volume lacks
                    # <total> bytes.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        Close-LargePut
                        $parts = $arg.Split(' ')
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))
                        $total = [int64]$parts[1]
                        $io = Get-IoPath $path
                        $dir = [System.IO.Path]::GetDirectoryName($io)
                        if ($dir) { [void][System.IO.Directory]::CreateDirectory($dir) }
                        $free = [int64]::MaxValue
                        try { $free = (New-Object System.IO.DriveInfo ([System.IO.Path]::GetPathRoot([System.IO.Path]::GetFullPath($path)))).AvailableFreeSpace } catch { }
                        if ($free -lt $total) { throw 'disk-full' }
                        $temp = Get-IoPath ([System.IO.Path]::Combine($dir, '.bridgevm-part-' + [Guid]::NewGuid().ToString('N')))
                        $stream = [System.IO.File]::Open($temp, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
                        $time = ''
                        if ($parts.Length -ge 3) { $time = $parts[2] }
                        $script:BvLPut = @{ Stream = $stream; Path = $path; Io = $io; Temp = $temp; Total = $total; Written = [int64]0; Seq = 0; Time = $time }
                        Write-Line $h 'OK LPUTBEG' 'OK'
                    } catch {
                        Close-LargePut
                        Write-Line $h ('ERR LPUTBEG ' + (Get-PutError $_)) 'ERR'
                    }
                }
                'LPUTDATA' {
                    # LPUTDATA <seq> <chars>, then the payload line as PUTDATA -> OK LPUTDATA <seq>. The payload is
                    # read before any check, so a refused chunk still leaves the wire on a line boundary; a
                    # refused chunk ends the file.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        $p = $arg.Split(' ')
                        $seq = [int]$p[0]
                        $data = Read-DataPayload $h ([int]$p[1])
                        $put = $script:BvLPut
                        if ($null -eq $put) { throw 'no-open-put' }
                        if ($seq -ne $put.Seq) { throw 'seq' }
                        if ($put.Written + $data.Length -gt $put.Total) { throw 'past-total' }
                        $put.Stream.Write($data, 0, $data.Length)
                        $put.Written += [int64]$data.Length
                        $put.Seq = $put.Seq + 1
                        Write-Line $h ('OK LPUTDATA ' + $seq) 'OK'
                    } catch {
                        Close-LargePut
                        Write-Line $h ('ERR LPUTDATA ' + (Get-PutError $_)) 'ERR'
                    }
                }
                'LPUTEND' {
                    # LPUTEND <chunks> -> PUTOK <b64(path)> <bytes>: the count checked, the temporary flushed to
                    # disk and given the Mac's time, then renamed over <path> in one step.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        $put = $script:BvLPut
                        if ($null -eq $put) { throw 'no-open-put' }
                        if ([int]$arg -ne $put.Seq) { throw 'seq' }
                        if ($put.Written -ne $put.Total) { throw 'short' }
                        $put.Stream.Flush($true)
                        $put.Stream.Close()
                        Set-PutTime $put.Temp $put.Time
                        Move-Replace $put.Temp $put.Io
                        $script:BvLPut = $null
                        Write-Line $h ('PUTOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($put.Path)) + ' ' + $put.Written) 'PUTOK'
                    } catch {
                        Close-LargePut
                        Write-Line $h ('ERR LPUTEND ' + (Get-PutError $_)) 'ERR'
                    }
                }
                'LPUTABORT' {
                    # LPUTABORT -> OK LPUTABORT: the large file's temporary is deleted, its target untouched.
                    Close-LargePut
                    Write-Line $h 'OK LPUTABORT' 'OK'
                }
                'DROPCAPS' { if ($null -eq $script:BvDropStream) { Clear-DropStaging }; [void](Test-DropReader); Write-Line $h 'BVDROP_CAPS 1' 'CAPS' }
                'DROPDIR' {
                    # DROPDIR <b64(rel)> -> OK DROPDIR <b64(final name)>: a new folder below the Desktop.
                    try {
                        $target = Resolve-DropTarget (Get-DropRoot) ([System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg))) $true
                        [void][System.IO.Directory]::CreateDirectory((Get-IoPath $target))
                        Write-Line $h ('OK DROPDIR ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes([System.IO.Path]::GetFileName($target)))) 'OK'
                    } catch { Write-Line $h ('ERR DROPDIR ' + $_.Exception.Message) 'ERR' }
                }
                'DROPBEG' {
                    # DROPBEG <b64(rel)> <size> -> OK DROPBEG <b64(final name)>. Never an existing file:
                    # DROPCHUNK fills a staged copy (Get-DropStaging), DROPEND moves it to the name, and
                    # any failure deletes it.
                    try {
                        Close-DropStream $false
                        $parts = $arg.Split(' ')
                        $total = [int64]$parts[1]
                        $target = Resolve-DropTarget (Get-DropRoot) ([System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))) $false
                        $temp = [System.IO.Path]::Combine((Get-DropStaging $target), ([Guid]::NewGuid().ToString('N') + '.part'))
                        $script:BvDropStream = [System.IO.File]::Open($temp, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
                        $script:BvDropTemp = $temp
                        $script:BvDropPath = $target
                        $script:BvDropTotal = $total
                        $script:BvDropStart = [System.Diagnostics.Stopwatch]::GetTimestamp()
                        $script:BvDropWriteTicks = [int64]0
                        if ($script:BvDropReader -eq $true) { [BridgeVM.BvDropReader]::ReadTicks = 0; [BridgeVM.BvDropReader]::DecodeTicks = 0 }
                        Log ('DROP BEG ' + $target + ' size=' + $total)
                        Write-Line $h ('OK DROPBEG ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes([System.IO.Path]::GetFileName($target)))) 'OK'
                    } catch {
                        Close-DropStream $false
                        Write-Line $h ('ERR DROPBEG ' + $_.Exception.Message) 'ERR'
                    }
                }
                'DROPCHUNK' {
                    # DROPCHUNK <seq> <chars>, then the payload as its own line of <chars> base64
                    # characters -> OK DROPCHUNK <seq>. The payload is consumed before any check, so
                    # a refused chunk still leaves the wire on a line boundary.
                    try {
                        $p = $arg.Split(' ')
                        $dropSeq = [int64]$p[0]
                        $data = Read-DataPayload $h ([int]$p[1])
                        if ($null -eq $script:BvDropStream) { throw 'no-open-drop' }
                        if ($dropSeq -ne $script:BvDropSeq) { throw 'seq' }
                        if ($script:BvDropWritten + $data.Length -gt $script:BvDropTotal) { throw 'too-long' }
                        $t0 = [System.Diagnostics.Stopwatch]::GetTimestamp()
                        $script:BvDropStream.Write($data, 0, $data.Length)
                        $script:BvDropWriteTicks += [System.Diagnostics.Stopwatch]::GetTimestamp() - $t0
                        $script:BvDropWritten += [int64]$data.Length
                        $script:BvDropSeq++
                        Write-DropLine $h ('OK DROPCHUNK ' + $dropSeq)
                    } catch {
                        Close-DropStream $false
                        Write-DropLine $h ('ERR DROPCHUNK ' + $_.Exception.Message)
                    }
                }
                'DROPEND' {
                    # DROPEND <chunks> -> OK DROPEND <bytes> once every byte is written.
                    try {
                        if ($null -eq $script:BvDropStream) { throw 'no-open-drop' }
                        if ([int64]$arg -ne $script:BvDropSeq) { throw 'seq' }
                        if ($script:BvDropWritten -ne $script:BvDropTotal) { throw 'short' }
                        # On disk before it has its name, so a crash never shows a cut-short file there.
                        $script:BvDropStream.Flush($true)
                        $script:BvDropStream.Close()
                        $script:BvDropStream = $null
                        # The name was free at DROPBEG; a file made there since keeps it.
                        $final = $script:BvDropPath
                        if ([System.IO.File]::Exists((Get-IoPath $final)) -or [System.IO.Directory]::Exists((Get-IoPath $final))) {
                            $cut = $final.LastIndexOf([char]92)
                            $final = Get-FreeDropPath $final.Substring(0, $cut) $final.Substring($cut + 1) $false
                        }
                        [System.IO.File]::Move($script:BvDropTemp, (Get-IoPath $final))
                        $script:BvDropTemp = ''
                        $written = $script:BvDropWritten
                        $tick = [double][System.Diagnostics.Stopwatch]::Frequency / 1000
                        $readMs = 0; $decodeMs = 0
                        if ($script:BvDropReader -eq $true) { $readMs = [BridgeVM.BvDropReader]::ReadTicks / $tick; $decodeMs = [BridgeVM.BvDropReader]::DecodeTicks / $tick }
                        Log ('DROP END {0} bytes={1} chunks={2} ms={3:F0} read_ms={4:F0} decode_ms={5:F0} write_ms={6:F0}' -f $final, $written, $script:BvDropSeq, (([System.Diagnostics.Stopwatch]::GetTimestamp() - $script:BvDropStart) / $tick), $readMs, $decodeMs, ($script:BvDropWriteTicks / $tick))
                        Close-DropStream $true
                        Write-Line $h ('OK DROPEND ' + $written) 'OK'
                    } catch {
                        Close-DropStream $false
                        Write-Line $h ('ERR DROPEND ' + $_.Exception.Message) 'ERR'
                    }
                }
                'DROPABORT' { Close-DropStream $false; Write-Line $h 'OK DROPABORT' 'OK' }
                'DEL' {
                    # DEL <b64(path)> deletes files only. Directories are never
                    # removed recursively by the shared-folder protocol.
                    $script:BvLsrChanges = [long]-1  # our own write: the next LSR lists afresh (share watch)
                    try {
                        $path = Get-IoPath ([System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg)))
                        if (Test-Path -LiteralPath $path -PathType Container) {
                            Write-Line $h 'ERR DEL is-dir' 'ERR'
                        } elseif (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
                            Write-Line $h 'OK DEL absent' 'OK'
                        } else {
                            Remove-Item -LiteralPath $path -Force -ErrorAction Stop
                            Write-Line $h 'OK DEL' 'OK'
                        }
                    } catch { Write-Line $h ('ERR DEL ' + $_.Exception.Message) 'ERR' }
                }
                default { Write-Line $h ("OUT 255 " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("unknown token: $tok"))) 'OUT' }
            }
        }
    } catch {
        Log ("loop error: " + $_.Exception.Message)
        Start-Sleep -Milliseconds 200
    }
}
