import Foundation

/// viogpudo's hardware cursor. The driver reads `HWCursor` (REG_DWORD) once, when it starts, from the
/// display adapter's driver key, `HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-...}\NNNN`
/// (IoOpenDeviceRegistryKey, PLUGPLAY_REGKEY_DRIVER), where its INF writes 0. With 1 the pointer is a
/// sprite the virtio-gpu device publishes to display.cursor and the app draws at the host pointer,
/// instead of a software cursor that costs a full guest frame per pointer step. A value set now applies
/// from the next Windows start.
///
/// The value can only be written once the 1AF4:1050 device has its driver: a first install usually
/// stages the driver for a device the VM gets at its next start. So the session sends this command at
/// `BVAGENT SERVICE start` on 2D virtio-gpu boots whose device has taken no cursor image (the first
/// boot with the device, and every VM installed before), and the install action sends it after an
/// `installed` result (a driver update rewrites the INF's 0).
extension HvfDisplayDriverInstall {
    /// One agent line that prints `BVCURSOR result=<set|already|no-device|failed> [detail=...]`.
    static let hardwareCursorCommand: String = {
        let script = #"""
            $ErrorActionPreference = 'Stop'
            try {
              $devices = @(Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -like 'PCI\VEN_1AF4&DEV_1050*' -and $_.Service -eq 'VioGpuDod' })
              if ($devices.Count -eq 0) { 'BVCURSOR result=no-device'; exit }
              $result = 'already'
              foreach ($device in $devices) {
                $key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\' + (Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName DEVPKEY_Device_Driver).Data
                if ((Get-ItemProperty -LiteralPath $key).HWCursor -ne 1) {
                  Set-ItemProperty -LiteralPath $key -Name HWCursor -Value 1 -Type DWord
                  $result = 'set'
                }
              }
              'BVCURSOR result=' + $result
            } catch { 'BVCURSOR result=failed detail=' + (($_.Exception.Message -replace '\s+', ' ').Trim()) }
            """#
        let encoded = Data(script.utf16.flatMap { [UInt8($0 & 0xff), UInt8($0 >> 8)] }).base64EncodedString()
        return "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand \(encoded)"
    }()
}

extension HvfEngineSession {
    /// At `BVAGENT SERVICE start`: a VM on the 2D virtio-gpu whose device took no cursor image in this
    /// boot still draws a software cursor, so switch the hardware cursor on for its next start. Once it
    /// is on, the device has an image by the time the agent starts and nothing is sent.
    func enableGuestHardwareCursorIfNeeded() {
        #if canImport(AppKit)
        guard config.virtioGpu2d, connectionState.guestCursorIsLive,
              !HvfGuestCursorFile.hasShapeThisBoot(path: HvfGuestCursorFile.path(evidenceDir: config.evidenceDir)),
              case let .success(command) = HvfGuestCommand.normalizeShell(HvfDisplayDriverInstall.hardwareCursorCommand)
        else { return }
        sendCtl(command)
        #endif
    }
}
