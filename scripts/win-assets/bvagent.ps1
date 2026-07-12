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
                'CLIPSET' {
                    # Set the guest Windows clipboard from CLIPSET <base64(utf8)>.
                    # Refresh the CLIPGET cache to the text we just wrote so the
                    # following poll serves it without reopening the clipboard.
                    try {
                        $txt = if ([string]::IsNullOrEmpty($arg)) { '' } else { [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($arg)) }
                        Set-Clipboard -Value $txt -ErrorAction SilentlyContinue
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
