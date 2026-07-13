@echo off
setlocal EnableExtensions

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" (
  echo BridgeVM build failed: vswhere.exe is missing.
  exit /b 10
)

set "VSROOT="
for /f "usebackq delims=" %%V in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath`) do set "VSROOT=%%V"
if not defined VSROOT (
  echo BridgeVM build failed: Visual Studio ARM64 tools are missing.
  exit /b 11
)

set "PYROOT="
for /d %%P in ("%LOCALAPPDATA%\Programs\Python\Python312*") do set "PYROOT=%%~fP"
if not defined PYROOT (
  echo BridgeVM build failed: Python 3.12 is missing.
  exit /b 12
)

set "PATH=C:\Program Files\Git\cmd;C:\Program Files\LLVM\bin;%PYROOT%;%PYROOT%\Scripts;%~dp0winflexbison;%PATH%"
call "%VSROOT%\Common7\Tools\VsDevCmd.bat" -arch=arm64 -host_arch=x64
if errorlevel 1 exit /b 13

"%PYROOT%\python.exe" -m pip install --disable-pip-version-check meson==1.7.2 ninja==1.11.1.4 packaging==25.0 mako==1.3.10 PyYAML==6.0.2
if errorlevel 1 exit /b 14

set "WORKDIR=C:\BridgeVMSubmitTraceBuild"
set "PACKAGE=%WORKDIR%\bridgevm-viogpu3d-arm64-package"
set "MANIFEST=%WORKDIR%\bridgevm-viogpu3d-arm64-package-pre-finalization.sha256"
set "FINALIZED=%WORKDIR%\bridgevm-viogpu3d-arm64-package-finalized"
set "ARCHIVE=%~dp0bridgevm-viogpu3d-submit-trace-finalized.zip"

rem A pinned prebuilt viogpu3d.sys beside this kit selects the UMD-only rebuild
rem mode, which does not need the WDK Visual Studio MSBuild toolsets (MSB8020).
if exist "%~dp0viogpu3d.sys" (
  powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0build-viogpu3d-arm64.ps1" -WorkDir "%WORKDIR%" -OutputDir "%PACKAGE%" -DriverSysPath "%~dp0viogpu3d.sys"
) else (
  powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0build-viogpu3d-arm64.ps1" -WorkDir "%WORKDIR%" -OutputDir "%PACKAGE%"
)
if errorlevel 1 exit /b 20

powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0finalize-viogpu3d-test-package.ps1" -PackageDir "%PACKAGE%" -PreFinalizationManifest "%MANIFEST%" -FinalizedDir "%FINALIZED%" -Finalizer "%~dp0finalize-viogpu3d-package.ps1"
if errorlevel 1 exit /b 21

if exist "%ARCHIVE%" del /f "%ARCHIVE%"
powershell.exe -NoLogo -NoProfile -NonInteractive -Command "Compress-Archive -Path '%FINALIZED%\*' -DestinationPath '%ARCHIVE%' -CompressionLevel Optimal"
if errorlevel 1 exit /b 22

echo BridgeVM submit-trace package ready: %ARCHIVE%
exit /b 0
