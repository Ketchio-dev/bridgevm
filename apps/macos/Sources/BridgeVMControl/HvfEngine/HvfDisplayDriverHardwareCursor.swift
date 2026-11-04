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
/// `installed` result (a driver update rewrites the INF's 0). The VM's `hardwareCursor` setting
/// (HvfHardwareCursorToggle.swift) turned off writes 0 instead and never 1.
extension HvfDisplayDriverInstall {
    /// One agent line that prints `BVCURSOR result=<set|already|no-device|failed> [detail=...]`.
    static let hardwareCursorCommand = hardwareCursorCommand(value: 1)
    /// The same lookup writing 0: Windows draws its own pointer again from its next start.
    static let hardwareCursorOffCommand = hardwareCursorCommand(value: 0)

    /// `$want` comes first, outside the script, which scripts/perf/vm_bench.py runs as `$want = 1`.
    private static func hardwareCursorCommand(value: Int) -> String {
        let script = "$want = \(value)\n" + #"""
            $ErrorActionPreference = 'Stop'
            try {
              $devices = @(Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -like 'PCI\VEN_1AF4&DEV_1050*' -and $_.Service -eq 'VioGpuDod' })
              if ($devices.Count -eq 0) { 'BVCURSOR result=no-device'; exit }
              $result = 'already'
              foreach ($device in $devices) {
                $key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\' + (Get-PnpDeviceProperty -InstanceId $device.InstanceId -KeyName DEVPKEY_Device_Driver).Data
                if ((Get-ItemProperty -LiteralPath $key).HWCursor -ne $want) {
                  Set-ItemProperty -LiteralPath $key -Name HWCursor -Value $want -Type DWord
                  $result = 'set'
                }
              }
              'BVCURSOR result=' + $result
            } catch { 'BVCURSOR result=failed detail=' + (($_.Exception.Message -replace '\s+', ' ').Trim()) }
            """#
        let encoded = Data(script.utf16.flatMap { [UInt8($0 & 0xff), UInt8($0 >> 8)] }).base64EncodedString()
        return "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand \(encoded)"
    }
}

extension HvfEngineSession {
    /// The VM's saved choice; nil (never set) is on.
    var guestHardwareCursorWanted: Bool { config.libraryContext?.config.hardwareCursor != false }

    /// At `BVAGENT SERVICE start` on the 2D virtio-gpu:
    /// - on: a boot whose device took no cursor image still draws a software cursor, so switch the
    ///   hardware cursor on for the next start. Once it is on, the device has an image by the time the
    ///   agent starts and nothing is sent.
    /// - off: write 0 once, at the first agent start after the setting changed or the app started. The
    ///   device's image flag cannot decide this: Windows may set its first pointer shape only after the
    ///   agent starts, and a missed write would leave the hardware cursor on. The guest writes only a
    ///   value that differs and otherwise prints `already`.
    func applyGuestHardwareCursorSetting() {
        #if canImport(AppKit)
        guard config.virtioGpu2d, connectionState.guestCursorIsLive else { return }
        let wanted = guestHardwareCursorWanted
        if wanted {
            hardwareCursorOffSent = false
            guard !HvfGuestCursorFile.hasShapeThisBoot(path: HvfGuestCursorFile.path(evidenceDir: config.evidenceDir))
            else { return }
        } else if hardwareCursorOffSent {
            return
        }
        let command = wanted ? HvfDisplayDriverInstall.hardwareCursorCommand : HvfDisplayDriverInstall.hardwareCursorOffCommand
        guard case let .success(line) = HvfGuestCommand.normalizeShell(command), sendCtl(line) else { return }
        if !wanted { hardwareCursorOffSent = true }
        #endif
    }
}
