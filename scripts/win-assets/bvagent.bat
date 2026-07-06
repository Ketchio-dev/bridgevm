@echo off
rem BridgeVM agent launcher — dropped in the all-users Startup folder so it runs
rem at logon regardless of the HKLM Run key. Launches the PowerShell agent.
start "" /min powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\bvagent.ps1
