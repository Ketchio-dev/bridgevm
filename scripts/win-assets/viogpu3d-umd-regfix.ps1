# BridgeVM viogpu3d Code-43 (CM_PROB_FAILED_POST_START) fix.
# The CI viogpu3d.inf is a stripped fallback missing the WDDM UMD registration,
# so dxgkrnl AddAdapter fails (STATUS_OBJECT_NAME_NOT_FOUND). This writes the
# missing UMD values into the adapter Class key. MUST run as SYSTEM (the key is
# TrustedInstaller-owned): schtasks /Create /RU SYSTEM ... /Run.
$ErrorActionPreference="SilentlyContinue"
$root="HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}"
$key=$null
Get-ChildItem $root | %{ $p=Get-ItemProperty $_.PSPath; if($p.MatchingDeviceId -match "ven_1af4&dev_1050"){ $key=$_.PSPath } }
$sys="$env:windir\System32"
Set-ItemProperty $key UserModeDriverName @("$sys\viogpu_d3d10.dll","$sys\viogpu_d3d10.dll","$sys\viogpu_d3d10.dll","$sys\viogpu_d3d10.dll") -Type MultiString
Set-ItemProperty $key OpenGLDriverName @("$sys\viogpu_wgl.dll") -Type MultiString
Set-ItemProperty $key OpenGLFlags 3 -Type DWord
Set-ItemProperty $key OpenGLVersion 4096 -Type DWord
Set-ItemProperty $key InstalledDisplayDrivers @("viogpu_d3d10","viogpu_d3d10","viogpu_d3d10") -Type MultiString
$id=(Get-PnpDevice|?{$_.InstanceId -match "VEN_1AF4&DEV_1050"}).InstanceId
Disable-PnpDevice -InstanceId $id -Confirm:$false; Start-Sleep 4; Enable-PnpDevice -InstanceId $id -Confirm:$false
