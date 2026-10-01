import Foundation
import XCTest
@testable import BridgeVMControl

/// The press outcome the display records for automation: where a click went.
final class HvfPointerPressOutcomeTests: XCTestCase {
    @MainActor
    private func withSession(negotiated: Bool, _ body: (HvfEngineSession) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("agent.ctl")
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true, ctlFilePath: control.path)
        let log = root.appendingPathComponent("run.log")
        try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(to: log, atomically: true, encoding: .utf8)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        XCTAssertTrue(session.attachToRunningVM())
        if negotiated {
            // Synthetic owned-boot boundary, not a real VM or Windows receipt.
            session.beginOwnedInputBoot()
            session.poll()
            let command = try String(contentsOf: control, encoding: .utf8).split(separator: "\n").last.map(String.init) ?? "missing"
            let id = command.split(separator: " ").last ?? "missing"
            let handle = try FileHandle(forWritingTo: log)
            try handle.seekToEnd()
            try handle.write(contentsOf: Data("BVAGENT CMD \(command) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\nBVAGENT END \(command)\n".utf8))
            try handle.close()
            session.poll(); session.poll()
        }
        try body(session)
    }

    @MainActor
    func testANegotiatedPressIsQueuedAndALetterboxPressIsUnmapped() throws {
        try withSession(negotiated: true) { session in
            let size = CGSize(width: 800, height: 600)
            XCTAssertEqual(session.sendPointerPress(location: CGPoint(x: 400, y: 300), viewSize: size, imageSize: size), "queued")
            XCTAssertEqual(session.sendPointerPress(location: CGPoint(x: 5, y: 300), viewSize: CGSize(width: 1000, height: 600),
                                                    imageSize: size), "unmapped")
        }
    }

    @MainActor
    func testAnUnknownAttachmentPressGoesToTheLegacyPath() throws {
        try withSession(negotiated: false) { session in
            let size = CGSize(width: 800, height: 600)
            XCTAssertEqual(session.sendPointerPress(location: CGPoint(x: 400, y: 300), viewSize: size, imageSize: size), "legacy")
        }
    }
}
