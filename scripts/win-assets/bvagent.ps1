# BridgeVM guest agent — runs host commands over the virtio-serial port.
# No compiled binary: PowerShell + kernel32/cfgmgr32 P/Invoke. Uses RAW IntPtr
# handles (SafeFileHandle mis-reports INVALID_HANDLE_VALUE as valid on
# Win-ARM64 / PS 5.1) with a manual INVALID_HANDLE_VALUE check. The port is
# opened by discovering the vioser device-interface path (GUID_VIOSERIAL_PORT),
# not a fixed \\.\Global\<name> symlink (that symlink only exists if PORT_NAME
# was processed, and was failing with GetLastError=203). Line protocol (ASCII,
# \n-terminated):
#   host->guest:  RUN <base64(utf8 cmd)>\n | PS <base64(utf8 script)>\n | PING\n
#   guest->host:  READY <hostname>\n | OUT <exit> <base64(utf8 out)>\n | PONG\n
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
#   CHAR Name[1]; } — default packing: Id@0-3, OutVqFull@4, HostConnected@5,
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
# REG_MULTI_SZ of \\?\... paths; simpler than SetupDi* — no interop structs).
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
    $written = 0
    $ok = $K::WriteFile($handle, $data, [uint32]$data.Length, [ref]$written, [IntPtr]::Zero)
    $gle = Gle
    # Evidence for the HostConnected question: ok=True with written=want yet no
    # host wire traffic (queue[5] notify=0) means vioser's WillWriteBlock
    # silently consumed the write because HostConnected is false.
    Log "WriteFile[$what] ok=$ok written=$written want=$($data.Length) gle=$gle"
}
function Write-Line($handle, [string]$line, [string]$what) {
    Write-Bytes $handle ([System.Text.Encoding]::ASCII.GetBytes($line + "`n")) $what
}
function Read-Line($handle) {
    # Full line, or $null after ~2s idle (only when no partial line buffered) so
    # the caller can poll port state.
    $sb = New-Object System.Text.StringBuilder
    $one = New-Object byte[] 1
    $spins = 0
    while ($true) {
        if (Is-BadHandle $handle) { return $null }
        $read = 0
        $ok = $K::ReadFile($handle, $one, 1, [ref]$read, [IntPtr]::Zero)
        if (-not $ok -or $read -eq 0) {
            if ($sb.Length -eq 0) {
                $spins++
                if ($spins -ge 133) { return $null }
            }
            Start-Sleep -Milliseconds 15
            continue
        }
        $spins = 0
        $b = $one[0]
        if ($b -eq 10) { break }        # \n
        if ($b -eq 13) { continue }     # \r
        [void]$sb.Append([char]$b)
    }
    return $sb.ToString()
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

Write-Line $h ("READY " + $env:COMPUTERNAME) 'READY'
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
                'RUN'  { $r = Invoke-B64 $arg $false; Write-Line $h ("OUT " + $r.Exit + " " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($r.Out))) 'OUT' }
                'PS'   { $r = Invoke-B64 $arg $true;  Write-Line $h ("OUT " + $r.Exit + " " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($r.Out))) 'OUT' }
                default { Write-Line $h ("OUT 255 " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("unknown token: $tok"))) 'OUT' }
            }
        }
    } catch {
        Log ("loop error: " + $_.Exception.Message)
        Start-Sleep -Milliseconds 200
    }
}
