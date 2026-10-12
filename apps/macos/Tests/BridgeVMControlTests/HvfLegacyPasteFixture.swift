#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// An attached legacy-input session whose non-ASCII text becomes a guest clipboard
/// command and a delayed `KEY ctrl+v` once the command's marker is acknowledged.
@MainActor
protocol HvfLegacyPasteFixture {}

extension HvfLegacyPasteFixture {
    func withSession(_ body: (HvfEngineSession, URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        try "BVAGENT READY host=synthetic t=1\nBVAGENT SERVICE start t=2\n".write(
            to: root.appendingPathComponent("run.log"), atomically: true, encoding: .utf8)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        XCTAssertTrue(session.attachToRunningVM())
        defer { session.closeLiveInput() }
        try body(session, root)
    }

    func lastCommand(_ session: HvfEngineSession) throws -> String {
        try XCTUnwrap(String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
            .split(separator: "\n").last.map(String.init))
    }

    func acknowledge(_ command: String, root: URL) throws {
        let range = try XCTUnwrap(command.range(of: "BVPASTE_READY [0-9A-Fa-f-]{36}", options: .regularExpression))
        let text = "BVAGENT CMD \(command) exit=0\n\(command[range])\nBVAGENT END \(command)\n"
        let handle = try FileHandle(forWritingTo: root.appendingPathComponent("run.log"))
        defer { try? handle.close() }
        try handle.seekToEnd(); try handle.write(contentsOf: Data(text.utf8))
    }
}
#endif
