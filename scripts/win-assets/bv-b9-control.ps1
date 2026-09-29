[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Firstboot','Foreground','Launch','Shutdown')][string]$Action,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$ExpectedControlSha256,
    [long]$Hwnd = 0,
    [string]$Nonce = '',
    [string]$ExpectedGuestScriptSha256 = '',
    [string]$ExpectedD3D11UmdSha = ''
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$share = 'C:\BridgeVMB9'
if ((Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedControlSha256) {
    throw 'B9 control script differs from the sealed source'
}
if ($Action -eq 'Firstboot') {
    $stage = Test-Path -LiteralPath 'C:\BridgeVM\stage3.flag'
    schtasks.exe /Query /TN BridgeVM-VioGpu3DFirstBoot *> $null
    if ($stage -and $LASTEXITCODE -ne 0) { Write-Output 'B9-FIRSTBOOT-READY' }
    else { Write-Output 'B9-FIRSTBOOT-PENDING' }
    return
}
if ($Action -eq 'Foreground') {
    if ($Hwnd -le 0) { throw 'invalid B9 target window' }
    Add-Type -Name Fg -Namespace B9 -MemberDefinition '[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();'
    Write-Output ('B9-FOREGROUND-' + [B9.Fg]::GetForegroundWindow().ToInt64())
    return
}
if ($Action -eq 'Launch') {
    if ($Nonce -cnotmatch '^[0-9a-f]{32}$' -or
        $ExpectedGuestScriptSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $ExpectedD3D11UmdSha -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid B9 launch identity' }
    $guest = Join-Path $share 'bv-b9-vlc-playback.ps1'
    $item = Get-Item -LiteralPath $guest -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'unsafe B9 guest script' }
    if ((Get-FileHash -LiteralPath $guest -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedGuestScriptSha256) {
        throw 'B9 guest script differs from the sealed source'
    }
    $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $guest -Nonce $Nonce -ExpectedD3D11UmdSha $ExpectedD3D11UmdSha -ExpectedGuestScriptSha256 $ExpectedGuestScriptSha256"
    $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $command }
    if ($created.ReturnValue -ne 0 -or $created.ProcessId -le 0) { throw 'B9 workload did not launch' }
    Write-Output "B9-WORKLOAD-LAUNCHED-$Nonce"
    return
}
shutdown.exe /s /f /t 0
