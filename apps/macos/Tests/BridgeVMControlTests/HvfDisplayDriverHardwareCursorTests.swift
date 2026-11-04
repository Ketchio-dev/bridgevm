import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfDisplayDriverHardwareCursorTests: XCTestCase {
    private typealias Install = HvfDisplayDriverInstall
    private let powershell = "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand "

    /// One short agent line, independent of install.ps1, that sets HWCursor=1 where viogpudo reads it.
    func testTheCommandSetsHWCursorInTheDisplayAdaptersDriverKey() throws {
        let command = Install.hardwareCursorCommand
        XCTAssertEqual(try HvfGuestCommand.normalizeShell(command).get(), command)
        XCTAssertTrue(command.utf8.count <= HvfClipboardPaste.maximumCommandCharacters, "\(command.utf8.count)")
        XCTAssertFalse(command.contains(where: { "\"%^|<>&".contains($0) }), "nothing cmd.exe would expand")
        XCTAssertTrue(command.hasPrefix(powershell))
        let script = try XCTUnwrap(Data(base64Encoded: String(command.dropFirst(powershell.count)))
            .flatMap { String(data: $0, encoding: .utf16LittleEndian) })
        for part in ["'PCI\\VEN_1AF4&DEV_1050*' -and $_.Service -eq 'VioGpuDod'",
                     "-KeyName DEVPKEY_Device_Driver",
                     "'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Class\\' + ",
                     "Set-ItemProperty -LiteralPath $key -Name HWCursor -Value 1 -Type DWord",
                     "'BVCURSOR result=no-device'", "'BVCURSOR result=' + $result",
                     "'BVCURSOR result=failed detail='"] {
            XCTAssertTrue(script.contains(part), part)
        }
        XCTAssertTrue(script.unicodeScalars.allSatisfy(\.isASCII))
    }

    /// Sent at the agent's start only on a 2D virtio-gpu boot whose device took no cursor image.
    func testServiceStartSendsItOnlyForAVirtioGpuBootWithoutACursorImage() throws {
        let cases: [(String, Bool, HvfGuestCursorFixture?, Bool)] = [
            ("no cursor file", true, nil, true),
            ("an earlier boot's image", true, HvfGuestCursorFixture(shapeThisBoot: false), true),
            ("an image this boot", true, HvfGuestCursorFixture(visible: false, shapeThisBoot: true), false),
            ("ramfb only", false, nil, false),
        ]
        for (label, virtioGpu2d, cursor, expected) in cases {
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("hw-cursor-" + UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
            defer { try? FileManager.default.removeItem(at: root) }
            let control = root.appendingPathComponent("agent.ctl")
            try cursor?.write(to: root.appendingPathComponent("display.cursor"))
            var config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
                evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false,
                shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
                nvmeBufferedIO: false, ctlFilePath: control.path)
            config.virtioGpu2d = virtioGpu2d
            let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
            session.connectionState = .connected(host: "guest")
            session.handle(.serviceStart(tMs: 1))
            let lines = ((try? String(contentsOf: control, encoding: .utf8)) ?? "").split(separator: "\n")
            XCTAssertEqual(lines.map(String.init), expected ? [Install.hardwareCursorCommand] : [], label)
        }
    }
}
