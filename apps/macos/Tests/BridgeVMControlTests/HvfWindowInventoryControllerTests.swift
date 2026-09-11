import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowInventoryControllerTests: XCTestCase {
    @MainActor
    func testSessionDispatchPublishesCompleteInventoryAndClearsOnRestart() throws {
        try withSession(service: true) { session, root in
            let model = HvfWindowInventoryController(session: session)
            XCTAssertTrue(model.refresh())
            XCTAssertTrue(model.isLoading)
            let control = root.appendingPathComponent("agent.ctl")
            let command = try String(contentsOf: control, encoding: .utf8).trimmingCharacters(in: .newlines)
            try append("BVAGENT \(command) WIN 42 7 0 0 640 480 QQ==\n", root: root)
            model.poll()
            XCTAssertTrue(model.records.isEmpty)
            try append("BVAGENT \(command) WINEND\n", root: root)
            model.poll()
            XCTAssertEqual(model.records.map(\.id), ["42"])
            XCTAssertFalse(model.isLoading)
            XCTAssertTrue(model.refresh())
            let next = try XCTUnwrap(String(contentsOf: control, encoding: .utf8).split(separator: "\n").last)
            try append("BVAGENT \(next) WIN 43 7 0 0 640 480 QQ==\n"
                + "BVAGENT \(next) WINEND\nBVAGENT READY host=restart t=3\n", root: root)
            model.poll()
            XCTAssertTrue(model.records.isEmpty)
            XCTAssertFalse(model.isLoading)
            XCTAssertEqual(model.status, "Guest restarted")
            model.stop()
        }
    }

    @MainActor
    func testUnstartedServiceCannotQueueInventory() throws {
        try withSession(service: false) { session, root in
            let model = HvfWindowInventoryController(session: session)
            XCTAssertFalse(model.refresh())
            XCTAssertFalse(model.isLoading)
            XCTAssertTrue(model.records.isEmpty)
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("agent.ctl").path))
        }
    }

    @MainActor
    private func withSession(service: Bool, body: (HvfEngineSession, URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path
        )
        try ("BVAGENT READY host=test t=1\n" + (service ? "BVAGENT SERVICE start t=2\n" : "")).write(
            to: root.appendingPathComponent("run.log"), atomically: true, encoding: .utf8)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        XCTAssertTrue(session.attachToRunningVM())
        try body(session, root)
    }

    private func append(_ text: String, root: URL) throws {
        let handle = try FileHandle(forWritingTo: root.appendingPathComponent("run.log"))
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
