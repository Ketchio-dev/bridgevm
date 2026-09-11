import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfControlReadinessTests: XCTestCase {
    @MainActor
    func testControlWritesRequireServiceNotJustReady() throws {
        for service in [false, true] {
            let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: root) }
            let config = HvfEngineConfig(
                targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
                watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
                shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
                nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path
            )
            let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
            XCTAssertFalse(session.sendCtl("PING"))
            XCTAssertFalse(FileManager.default.fileExists(atPath: config.ctlFilePath))
            let log = "BVAGENT READY host=test t=1\n"
                + (service ? "BVAGENT SERVICE start t=2\n" : "")
            try log.write(to: root.appendingPathComponent("run.log"), atomically: true, encoding: .utf8)
            XCTAssertTrue(session.attachToRunningVM())
            XCTAssertEqual(session.sendCtl("PING"), service)
            if service {
                XCTAssertEqual(try String(contentsOfFile: config.ctlFilePath, encoding: .utf8), "PING\n")
            } else {
                XCTAssertFalse(FileManager.default.fileExists(atPath: config.ctlFilePath))
            }
        }
    }
}
