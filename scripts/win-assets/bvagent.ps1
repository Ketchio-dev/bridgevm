# BridgeVM guest agent — runs host commands over the virtio-serial port.
# No compilation needed: pure PowerShell + .NET, driven by the in-box vioser
# driver's named port. Line protocol (ASCII, newline-terminated):
#   host -> guest:  RUN <base64(utf8 command)>\n      (run via cmd /c)
#                   PS  <base64(utf8 script)>\n        (run via powershell)
#                   PING\n
#   guest -> host:  READY <hostname>\n                 (on connect)
#                   OUT <exit> <base64(utf8 stdout+stderr)>\n
#                   PONG\n
# The host frames replies by the leading token; base64 keeps binary/newlines safe.

$ErrorActionPreference = 'Continue'
$portPath = '\\.\Global\org.bridgevm.agent.0'

function Open-Port {
    for ($i = 0; $i -lt 120; $i++) {
        try {
            return New-Object System.IO.FileStream(
                $portPath,
                [System.IO.FileMode]::Open,
                [System.IO.FileAccess]::ReadWrite,
                [System.IO.FileShare]::ReadWrite)
        } catch {
            Start-Sleep -Milliseconds 500
        }
    }
    throw "bvagent: could not open $portPath"
}

function Write-Line($stream, [string]$line) {
    $bytes = [System.Text.Encoding]::ASCII.GetBytes($line + "`n")
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush()
}

function Read-Line($stream) {
    $sb = New-Object System.Text.StringBuilder
    while ($true) {
        $b = $stream.ReadByte()
        if ($b -lt 0) { Start-Sleep -Milliseconds 20; continue }
        if ($b -eq 10) { break }          # \n
        if ($b -eq 13) { continue }       # \r
        [void]$sb.Append([char]$b)
    }
    return $sb.ToString()
}

function Invoke-B64([string]$b64, [bool]$usePwsh) {
    $cmd = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($b64))
    $tmpOut = [System.IO.Path]::GetTempFileName()
    $tmpErr = [System.IO.Path]::GetTempFileName()
    if ($usePwsh) {
        $scriptFile = [System.IO.Path]::GetTempFileName() + '.ps1'
        Set-Content -Path $scriptFile -Value $cmd -Encoding UTF8
        $p = Start-Process -FilePath 'powershell.exe' `
            -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',$scriptFile) `
            -RedirectStandardOutput $tmpOut -RedirectStandardError $tmpErr -NoNewWindow -Wait -PassThru
        Remove-Item $scriptFile -ErrorAction SilentlyContinue
    } else {
        $p = Start-Process -FilePath 'cmd.exe' -ArgumentList @('/c', $cmd) `
            -RedirectStandardOutput $tmpOut -RedirectStandardError $tmpErr -NoNewWindow -Wait -PassThru
    }
    $out = (Get-Content -Path $tmpOut -Raw -ErrorAction SilentlyContinue) + `
           (Get-Content -Path $tmpErr -Raw -ErrorAction SilentlyContinue)
    Remove-Item $tmpOut, $tmpErr -ErrorAction SilentlyContinue
    if ($null -eq $out) { $out = '' }
    return @{ Exit = $p.ExitCode; Out = $out }
}

$stream = Open-Port
Write-Line $stream ("READY " + $env:COMPUTERNAME)
while ($true) {
    $line = Read-Line $stream
    if ([string]::IsNullOrWhiteSpace($line)) { continue }
    $sp = $line.IndexOf(' ')
    $tok = if ($sp -lt 0) { $line } else { $line.Substring(0, $sp) }
    $arg = if ($sp -lt 0) { '' } else { $line.Substring($sp + 1) }
    switch ($tok) {
        'PING' { Write-Line $stream 'PONG' }
        'RUN'  {
            $r = Invoke-B64 $arg $false
            $b64 = [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($r.Out))
            Write-Line $stream ("OUT " + $r.Exit + " " + $b64)
        }
        'PS'   {
            $r = Invoke-B64 $arg $true
            $b64 = [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($r.Out))
            Write-Line $stream ("OUT " + $r.Exit + " " + $b64)
        }
        default { Write-Line $stream ("OUT 255 " + [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("unknown token: $tok"))) }
    }
}
