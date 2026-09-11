import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfControlWriteFailureTests: XCTestCase {
    @MainActor
    func testControlInputReportsWriteFailure() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let invalidControlPath = temp.appendingPathComponent("control-directory", isDirectory: true)
        try FileManager.default.createDirectory(at: invalidControlPath, withIntermediateDirectories: true)
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: temp.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: invalidControlPath.path
        )
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in true }
        try "BVAGENT SERVICE start t=1\n".write(
            to: temp.appendingPathComponent("run.log"), atomically: true, encoding: .utf8
        )
        XCTAssertTrue(session.attachToRunningVM())
        XCTAssertFalse(session.sendCtl("whoami"))
        XCTAssertTrue(session.events.contains { event in
            if case let .unknown(message) = event {
                return message.hasPrefix("control command write failed:")
            }
            return false
        })
    }
}
