@echo off
rem BridgeVM first-boot GPU driver activation. Runs ONCE, elevated, on the live
rem Windows OS via an HKLM RunOnce entry planted by bvinject.cmd (UAC is disabled
rem in the dev/build image, so RunOnce gets the full admin token).
rem
rem WHY THIS EXISTS: offline `dism /Add-Driver` only STAGES viogpu3d into the
rem DriverStore. It does NOT (a) trust the self-signed test publisher, so silent
rem PnP install of the test-signed third-party package is refused, nor (b) re-run
rem a driver search against the already-present virtio-gpu device. BCD
rem `testsigning on` only lets the kernel LOAD test-signed code once installed; it
rem does not make PnP TRUST the publisher for install. So we must, on the live OS:
rem   1) import the test cert into the machine Root + TrustedPublisher stores, and
rem   2) force pnputil to add+install the package so it binds PCI\VEN_1AF4&DEV_1050.
rem Everything is logged to C:\BridgeVM\viogpu3d-firstboot.log so the next offline
rem image extraction shows whether the bind succeeded or what problem remains.
rem
rem NOTE: a space precedes every redirection operator on purpose. %TIME% ends in a
rem digit, and `<digit>>file` is parsed by cmd as a numbered-stream redirect.
setlocal
set PKG=C:\BridgeVM\viogpu3d
set CER=%PKG%\BridgeVM-viogpu3d-Test.cer
set LOG=C:\BridgeVM\viogpu3d-firstboot.log

echo [bvgpu-firstboot] start %DATE% %TIME% > "%LOG%"

rem Live-set testsigning as a belt-and-suspenders: offline bvinject sets it on the
rem target BCD when it can reach the ESP, but that can fail in WinPE (no ESP drive
rem letter). The kernel only LOADS a test-signed driver with testsigning active,
rem which needs a reboot to take effect — handled by the guarded reboot below.
echo [bvgpu-firstboot] bcdedit testsigning on >> "%LOG%"
bcdedit /set {current} testsigning on >> "%LOG%" 2>&1

echo [bvgpu-firstboot] trust test cert (Root) >> "%LOG%"
certutil -f -addstore Root "%CER%" >> "%LOG%" 2>&1
echo [bvgpu-firstboot] trust test cert (TrustedPublisher) >> "%LOG%"
certutil -f -addstore TrustedPublisher "%CER%" >> "%LOG%" 2>&1

echo [bvgpu-firstboot] pnputil /add-driver viogpu3d.inf /install >> "%LOG%"
pnputil /add-driver "%PKG%\viogpu3d.inf" /install >> "%LOG%" 2>&1

echo [bvgpu-firstboot] pnputil /scan-devices >> "%LOG%"
pnputil /scan-devices >> "%LOG%" 2>&1

echo [bvgpu-firstboot] pnputil /enum-devices (Display class, for diagnosis) >> "%LOG%"
pnputil /enum-devices /class Display >> "%LOG%" 2>&1

echo [bvgpu-firstboot] pnputil /enum-drivers (staged OEM infs) >> "%LOG%"
pnputil /enum-drivers >> "%LOG%" 2>&1

echo [bvgpu-firstboot] done %DATE% %TIME% >> "%LOG%"

rem Reboot ONCE so the freshly-enabled testsigning takes effect and the installed
rem viogpu3d.sys actually loads/binds. Guarded by a flag so this can never loop.
if not exist C:\BridgeVM\gpu-rebooted.flag (
  echo [bvgpu-firstboot] first pass done; rebooting to activate testsigning >> "%LOG%"
  echo rebooted > C:\BridgeVM\gpu-rebooted.flag
  shutdown /r /t 5 /c "BridgeVM viogpu3d activation reboot"
)
endlocal
