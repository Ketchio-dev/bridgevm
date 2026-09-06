$ErrorActionPreference = 'Stop'
$marker = 'C:\BridgeVMClosure\bv-notepad-started.log'
Remove-Item -LiteralPath $marker -Force -ErrorAction SilentlyContinue
$result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'notepad.exe' }
if ($result.ReturnValue -ne 0 -or $result.ProcessId -le 0) { throw 'Notepad CIM launch failed' }
Start-Sleep -Seconds 3
[IO.File]::WriteAllText($marker, "BVNOTEPADSTARTED pid=$($result.ProcessId)`r`n")
Write-Output "BVNOTEPADSTARTED pid=$($result.ProcessId)"
