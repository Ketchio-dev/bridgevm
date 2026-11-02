import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfControlCommandAppendIntegrationTests: XCTestCase {
    func testSessionKeepsServiceGateAndCreatesOnlyItsAdmittedControlFile() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("metadata/agent.ctl")
        let config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
            evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: control.path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        XCTAssertFalse(session.sendCtl("hostname"))
        XCTAssertFalse(FileManager.default.fileExists(atPath: control.path))
        session.serviceStarted = true
        XCTAssertFalse(session.sendCtl("hostname\nwhoami"))
        XCTAssertTrue(session.sendCtl("hostname"))
        XCTAssertTrue(session.sendCtl("whoami"))
        XCTAssertEqual(try String(contentsOf: control, encoding: .utf8), "hostname\nwhoami\n")
        try FileManager.default.removeItem(at: control)
        try FileManager.default.createDirectory(at: control, withIntermediateDirectories: false)
        XCTAssertFalse(session.sendCtl("ver"))
        XCTAssertTrue(session.events.contains { $0.displayText.contains("control command write failed:") })
    }

    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("control-append-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        return root
    }
}
