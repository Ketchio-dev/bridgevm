# Diagnostic-only guest asset for B6 scene-composition scoping. Not part of
# any shipped closure gate. Launches the packaged Microsoft.WindowsNotepad
# app by AUMID rather than notepad.exe, which resolves to the classic win32
# binary on this image even though the modern package is installed.
$ErrorActionPreference = 'Stop'
$pkg = Get-AppxPackage -Name Microsoft.WindowsNotepad -ErrorAction Stop
$aumid = "$($pkg.PackageFamilyName)!App"
$result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = "explorer.exe shell:AppsFolder\$aumid" }
if ($result.ReturnValue -ne 0) { throw 'Modern Notepad CIM launch failed' }
Start-Sleep -Seconds 4
Write-Output "BVMODERNNOTEPAD aumid=$aumid returnvalue=$($result.ReturnValue)"
