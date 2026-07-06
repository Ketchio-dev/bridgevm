# BridgeVM guest agent — runs host commands over the virtio-serial port.
# No compiled binary: PowerShell + kernel32 P/Invoke (CreateFile / ReadFile /
# WriteFile) directly on the device handle. .NET FileStream refuses device
# paths, so we do raw ReadFile/WriteFile. Line protocol (ASCII, \n-terminated):
#   host->guest:  RUN <base64(utf8 cmd)>\n | PS <base64(utf8 script)>\n | PING\n
#   guest->host:  READY <hostname>\n | OUT <exit> <base64(utf8 out)>\n | PONG\n
$ErrorActionPreference = 'Continue'
# Single-instance guard: the Run key and Startup launcher can both fire; a
# second opener would collide on the single-open virtio-serial port.
$bvCreated = $false
$bvMutex = New-Object System.Threading.Mutex($true, 'Global\BridgeVMAgentSingleton', [ref]$bvCreated)
if (-not $bvCreated) { exit 0 }
$portPath = '\\.\Global\org.bridgevm.agent.0'

$sig = @'
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
public static extern Microsoft.Win32.SafeHandles.SafeFileHandle CreateFile(
    string n, uint access, uint share, IntPtr sa, uint disp, uint flags, IntPtr templ);
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool ReadFile(Microsoft.Win32.SafeHandles.SafeFileHandle h,
    byte[] buf, uint toRead, out uint read, IntPtr overlapped);
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool WriteFile(Microsoft.Win32.SafeHandles.SafeFileHandle h,
    byte[] buf, uint toWrite, out uint written, IntPtr overlapped);
'@
$K = Add-Type -MemberDefinition $sig -Name 'BvKernel32' -Namespace 'BridgeVM' -PassThru

function Open-Port {
    for ($i = 0; $i -lt 120; $i++) {
        # GENERIC_READ|GENERIC_WRITE=0xC0000000, FILE_SHARE_READ|WRITE=3, OPEN_EXISTING=3
        $h = $K::CreateFile($portPath, 0xC0000000, 3, [IntPtr]::Zero, 3, 0, [IntPtr]::Zero)
        if (-not $h.IsInvalid) { return $h }
        $h.Dispose()
        Start-Sleep -Milliseconds 500
    }
    throw "bvagent: could not open $portPath"
}

function Write-Bytes($h, [byte[]]$data) {
    $written = 0
    [void]$K::WriteFile($h, $data, [uint32]$data.Length, [ref]$written, [IntPtr]::Zero)
}
function Write-Line($h, [string]$line) {
    Write-Bytes $h ([System.Text.Encoding]::ASCII.GetBytes($line + "`n"))
}
function Read-Line($h) {
    $sb = New-Object System.Text.StringBuilder
    $one = New-Object byte[] 1
    while ($true) {
        $read = 0
        $ok = $K::ReadFile($h, $one, 1, [ref]$read, [IntPtr]::Zero)
        if (-not $ok -or $read -eq 0) { Start-Sleep -Milliseconds 15; continue }
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

$h = Open-Port
Write-Line $h ("READY " + $env:COMPUTERNAME)
while ($true) {
    $line = Read-Line $h
    if ([string]::IsNullOrWhiteSpace($line)) { continue }
    $sp = $line.IndexOf(' ')
    $tok = if ($sp -lt 0) { $line } else { $line.Substring(0, $sp) }
    $arg = if ($sp -lt 0) { '' } else { $line.Substring($sp + 1) }
    switch ($tok) {
        'PING' { Write-Line $h 'PONG' }
        'RUN'  { $r = Invoke-B64 $arg $false; Write-Line $h ("OUT " + $r.Exit + " " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($r.Out))) }
        'PS'   { $r = Invoke-B64 $arg $true;  Write-Line $h ("OUT " + $r.Exit + " " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($r.Out))) }
        default { Write-Line $h ("OUT 255 " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("unknown token: $tok"))) }
    }
}
