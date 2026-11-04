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

function Log-PortInfo($handle, [string]$when) {
    $info = Get-PortInfoRaw $handle
    if ($null -eq $info) { Log "PORTINFO[$when] unavailable"; return }
    Log "PORTINFO[$when] id=$($info.Id) HostConnected=$($info.Host) GuestConnected=$($info.Guest) OutVqFull=$($info.OutVqFull) name='$($info.Name)'"
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

function Write-Bytes($handle, [byte[]]$data, [string]$what) {
    if (Is-BadHandle $handle) { Log "WriteFile[$what] SKIPPED bad handle"; return }
    $want = $data.Length
    $totalWritten = 0
    $remaining = $data
    while ($remaining.Length -gt 0) {
        $written = [uint32]0
        $ok = $K::WriteFile($handle, $remaining, [uint32]$remaining.Length, [ref]$written, [IntPtr]::Zero)
        $gle = Gle
        if (-not $ok -or $written -eq 0) {
            Log "WriteFile[$what] FAILED ok=$ok written=$written total=$totalWritten want=$want gle=$gle"
            return
        }
        $totalWritten += [int]$written
        if ($written -lt $remaining.Length) {
            # Synchronous WriteFile is allowed to complete only a prefix. Keep
            # writing the suffix; otherwise the missing newline leaves the host
            # protocol permanently waiting on a partial frame.
            $suffix = New-Object byte[] ($remaining.Length - [int]$written)
            [System.Array]::Copy($remaining, [int]$written, $suffix, 0, $suffix.Length)
            $remaining = $suffix
        } else {
            $remaining = New-Object byte[] 0
        }
    }
    # Evidence for the HostConnected question: ok=True with written=want yet no
    # host wire traffic (queue[5] notify=0) means vioser's WillWriteBlock
    # silently consumed the write because HostConnected is false.
    Log "WriteFile[$what] complete written=$totalWritten want=$want"
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

function Close-PutStream {
    if ($null -ne $script:BvPutStream) {
        try { $script:BvPutStream.Close() } catch { }
        $script:BvPutStream = $null
    }
    $script:BvPutPath = ''
    $script:BvPutTotal = [int64]0
    $script:BvPutWritten = [int64]0
    $script:BvPutSeq = 0
}

# Finder drops land on the logged-on user's Desktop through their own stream, apart from the
# shared folder's PUT stream, so share sync and a drop can take turns on the wire.
$script:BvDropStream = $null
$script:BvDropPath = ''
$script:BvDropTotal = [int64]0
$script:BvDropWritten = [int64]0
$script:BvDropSeq = [int64]0

function Close-DropStream([bool]$Keep) {
    if ($null -ne $script:BvDropStream) {
        try { $script:BvDropStream.Close() } catch { }
        $script:BvDropStream = $null
        if (-not $Keep -and $script:BvDropPath) { try { [System.IO.File]::Delete($script:BvDropPath) } catch { } }
    }
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
        static readonly byte[] Scratch = new byte[65536];
        public static long ReadTicks;
        public static long DecodeTicks;

        // Completes one base64 payload line of exactly `want` characters plus its newline. `prefix`
        // is what the line reader already holds; `rest` returns whatever arrived after the newline.
        public static byte[] ReadPayload(IntPtr h, string prefix, int want, int idleMs, out string rest) {
            long start = Stopwatch.GetTimestamp();
            byte[] text = new byte[want + 1];
            int have = Math.Min(prefix.Length, want + 1);
            for (int i = 0; i < have; i++) { text[i] = (byte)prefix[i]; }
            StringBuilder extra = new StringBuilder();
            if (prefix.Length > have) { extra.Append(prefix, have, prefix.Length - have); }
            long idleSince = 0;
            while (have < want + 1) {
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
                int take = Math.Min((int)read, want + 1 - have);
                Buffer.BlockCopy(Scratch, 0, text, have, take);
                have += take;
                if (take < (int)read) { extra.Append(Encoding.ASCII.GetString(Scratch, take, (int)read - take)); }
            }
            rest = extra.ToString();
            if (text[want] != (byte)'\n') { throw new FormatException("drop-payload-frame"); }
            long mid = Stopwatch.GetTimestamp();
            byte[] data = Convert.FromBase64String(Encoding.ASCII.GetString(text, 0, want));
            ReadTicks += mid - start;
            DecodeTicks += Stopwatch.GetTimestamp() - mid;
            return data;
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
# reparse point; an existing name becomes "name (2).ext" ("name (2)" for a folder).
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
        $attr = [System.IO.File]::GetAttributes($dir)
        if (($attr -band [System.IO.FileAttributes]::Directory) -eq 0 -or ($attr -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'bad-parent' }
    }
    $leaf = $parts[$parts.Length - 1]
    $stem = $leaf
    $ext = ''
    if (-not $IsDir) {
        $stem = [System.IO.Path]::GetFileNameWithoutExtension($leaf)
        $ext = [System.IO.Path]::GetExtension($leaf)
    }
    $target = [System.IO.Path]::Combine($dir, $leaf)
    $n = 2
    while ([System.IO.File]::Exists($target) -or [System.IO.Directory]::Exists($target)) {
        $target = [System.IO.Path]::Combine($dir, ('{0} ({1}){2}' -f $stem, $n, $ext))
        $n++
    }
    return $target
}

function Read-Line($handle) {
    # Full line, or $null after ~2s idle so the caller can poll port state.
    if ($script:BvLines.Count -gt 0) { return $script:BvLines.Dequeue() }
    $spins = 0
    while ($true) {
        if (Is-BadHandle $handle) { return $null }
        $read = 0
        $ok = $K::ReadFile($handle, $script:BvReadBuffer, [uint32]$script:BvReadBuffer.Length, [ref]$read, [IntPtr]::Zero)
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
$script:BvClipImageSource = @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;
namespace BridgeVM {
    public static class BvClipImage {
        public const int MaxBytes = 33554432;
        public const int MaxSide = 16384;

        static void OnSta(ThreadStart body) {
            Exception failure = null;
            Thread thread = new Thread(delegate () {
                try { body(); } catch (Exception e) { failure = e; }
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

Write-Line $h ("READY " + $env:COMPUTERNAME + " v3-share2") 'READY'
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
                'CLIPCAPS' {
                    # CLIPCAPS -> CLIPCAPS IMAGE_PNG <b64(dir)> when this agent can move
                    # clipboard images, else a bare CLIPCAPS (text only).
                    if (Initialize-ClipImage) {
                        Write-Line $h ('CLIPCAPS IMAGE_PNG ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script:BvClipDir))) 'CLIPCAPS'
                    } else { Write-Line $h 'CLIPCAPS' 'CLIPCAPS' }
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
                'CLIPSET' {
                    # Set the guest Windows clipboard from CLIPSET <base64(utf8)>.
                    # Refresh the CLIPGET cache to the text we just wrote so the
                    # following poll serves it without reopening the clipboard.
                    try {
                        $txt = if ([string]::IsNullOrEmpty($arg)) { '' } else { [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg)) }
                        Set-Clipboard -Value $txt -ErrorAction Stop
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
                        $sb = New-Object System.Text.StringBuilder
                        foreach ($e in Get-ChildItem -LiteralPath $rootFull -Recurse -Force -ErrorAction Stop) {
                            $full = [System.IO.Path]::GetFullPath($e.FullName)
                            if (-not $full.StartsWith($rootFull, [System.StringComparison]::OrdinalIgnoreCase)) { continue }
                            $rel = $full.Substring($rootFull.Length)
                            if ([string]::IsNullOrEmpty($rel)) { continue }
                            $sz = if ($e.PSIsContainer) { 0 } else { $e.Length }
                            [void]$sb.AppendLine(('{0}|{1}|{2}|{3}' -f $rel, $sz, [int]$e.PSIsContainer, $e.LastWriteTimeUtc.ToString('o')))
                        }
                        Write-Line $h ('LSOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($sb.ToString()))) 'LSOK'
                    } catch { Write-Line $h ('ERR LSR ' + $_.Exception.Message) 'ERR' }
                }
                'GET' {
                    # GET <b64(path)> -> GETBEG <b64(path)> <total> <nchunks>,
                    # then N x GETCHUNK <seq> <b64(rawbytes)>, then GETEND <seq>.
                    # Chunks are 24 KiB raw so each base64 line stays reasonable.
                    try {
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg))
                        $bytes = [System.IO.File]::ReadAllBytes($path)
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
                'PUT' {
                    # PUT <b64(path)> <b64(rawbytes)> -> PUTOK <b64(path)> <bytes>.
                    # Single-chunk write; the host sends the whole file in one line.
                    try {
                        $parts = $arg.Split(' ')
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))
                        $data = if ($parts.Length -ge 2 -and $parts[1]) { [System.Convert]::FromBase64String($parts[1]) } else { New-Object byte[] 0 }
                        $dir = [System.IO.Path]::GetDirectoryName($path)
                        if ($dir -and -not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir -Force) }
                        [System.IO.File]::WriteAllBytes($path, $data)
                        Write-Line $h ('PUTOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($path)) + ' ' + $data.Length) 'PUTOK'
                    } catch { Write-Line $h ('ERR PUT ' + $_.Exception.Message) 'ERR' }
                }
                'PUTBEG' {
                    # PUTBEG <b64(path)> <total> <nchunks> -> OK PUTBEG.
                    # Opens/truncates a stream; PUTCHUNK/PUTEND complete it.
                    try {
                        Close-PutStream
                        $parts = $arg.Split(' ')
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))
                        $total = [int64]$parts[1]
                        $dir = [System.IO.Path]::GetDirectoryName($path)
                        if ($dir -and -not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir -Force) }
                        $script:BvPutStream = [System.IO.File]::Open($path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
                        $script:BvPutPath = $path
                        $script:BvPutTotal = $total
                        $script:BvPutWritten = [int64]0
                        $script:BvPutSeq = 0
                        Write-Line $h 'OK PUTBEG' 'OK'
                    } catch {
                        Close-PutStream
                        Write-Line $h ('ERR PUTBEG ' + $_.Exception.Message) 'ERR'
                    }
                }
                'PUTCHUNK' {
                    # PUTCHUNK <seq> <b64(rawbytes)> -> OK PUTCHUNK <seq>.
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
                'PUTEND' {
                    # PUTEND <seq> -> PUTOK <b64(path)> <bytes> after byte-count
                    # verification. The terminal reply matches legacy PUT.
                    try {
                        if ($null -eq $script:BvPutStream) { throw 'no-open-put' }
                        $seq = [int]$arg
                        $path = $script:BvPutPath
                        $written = $script:BvPutWritten
                        $total = $script:BvPutTotal
                        $expectedSeq = $script:BvPutSeq
                        Close-PutStream
                        if ($seq -ne $expectedSeq) {
                            Write-Line $h 'ERR PUTEND seq' 'ERR'
                        } elseif ($written -ne $total) {
                            Write-Line $h 'ERR PUTEND short' 'ERR'
                        } else {
                            Write-Line $h ('PUTOK ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($path)) + ' ' + $written) 'PUTOK'
                        }
                    } catch {
                        Close-PutStream
                        Write-Line $h ('ERR PUTEND ' + $_.Exception.Message) 'ERR'
                    }
                }
                'DROPCAPS' { [void](Test-DropReader); Write-Line $h 'BVDROP_CAPS 1' 'CAPS' }
                'DROPDIR' {
                    # DROPDIR <b64(rel)> -> OK DROPDIR <b64(final name)>: a new folder below the Desktop.
                    try {
                        $target = Resolve-DropTarget (Get-DropRoot) ([System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg))) $true
                        [void][System.IO.Directory]::CreateDirectory($target)
                        Write-Line $h ('OK DROPDIR ' + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes([System.IO.Path]::GetFileName($target)))) 'OK'
                    } catch { Write-Line $h ('ERR DROPDIR ' + $_.Exception.Message) 'ERR' }
                }
                'DROPBEG' {
                    # DROPBEG <b64(rel)> <size> -> OK DROPBEG <b64(final name)>. Always a new file, never
                    # an existing one; DROPCHUNK/DROPEND fill it, and any failure deletes it.
                    try {
                        Close-DropStream $false
                        $parts = $arg.Split(' ')
                        $total = [int64]$parts[1]
                        $target = Resolve-DropTarget (Get-DropRoot) ([System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($parts[0]))) $false
                        $script:BvDropStream = [System.IO.File]::Open($target, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
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
                        $want = [int]$p[1]
                        if ($script:BvLines.Count -gt 0) {
                            $data = [System.Convert]::FromBase64String($script:BvLines.Dequeue())
                        } elseif (Test-DropReader) {
                            $rest = ''
                            $data = [BridgeVM.BvDropReader]::ReadPayload($h, $script:BvPending.ToString(), $want, 30000, [ref]$rest)
                            $script:BvPending.Length = 0
                            if ($rest.Contains("`n")) {
                                $restParts = $rest.Split([char]10)
                                for ($i = 0; $i -lt $restParts.Length - 1; $i++) { $script:BvLines.Enqueue($restParts[$i].TrimEnd([char]13)) }
                                $rest = $restParts[$restParts.Length - 1]
                            }
                            [void]$script:BvPending.Append($rest)
                        } else {
                            $payload = $null
                            for ($i = 0; $null -eq $payload -and $i -lt 15; $i++) { $payload = Read-Line $h }
                            $data = [System.Convert]::FromBase64String([string]$payload)
                        }
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
                        $script:BvDropStream.Flush()
                        $written = $script:BvDropWritten
                        $tick = [double][System.Diagnostics.Stopwatch]::Frequency / 1000
                        $readMs = 0; $decodeMs = 0
                        if ($script:BvDropReader -eq $true) { $readMs = [BridgeVM.BvDropReader]::ReadTicks / $tick; $decodeMs = [BridgeVM.BvDropReader]::DecodeTicks / $tick }
                        Log ('DROP END {0} bytes={1} chunks={2} ms={3:F0} read_ms={4:F0} decode_ms={5:F0} write_ms={6:F0}' -f $script:BvDropPath, $written, $script:BvDropSeq, (([System.Diagnostics.Stopwatch]::GetTimestamp() - $script:BvDropStart) / $tick), $readMs, $decodeMs, ($script:BvDropWriteTicks / $tick))
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
                    try {
                        $path = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg))
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
