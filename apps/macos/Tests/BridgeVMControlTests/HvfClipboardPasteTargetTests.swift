#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfClipboardPasteTargetTests: XCTestCase {
    func testTargetCancellationDropsDelayedLegacyPasteAndAllowsFreshRequest() throws {
        try withSession { session, root in
            XCTAssertEqual(session.sendText("한글"), .acceptedForProcessing)
            let old = try lastCommand(session)
            session.cancelOrderedInputTarget()
            try acknowledge(old, root: root)
            session.poll()
            let input = root.appendingPathComponent("input.ctl")
            XCTAssertFalse(FileManager.default.fileExists(atPath: input.path))
            XCTAssertEqual(session.sendText("새 입력"), .acceptedForProcessing)
            let fresh = try lastCommand(session)
            XCTAssertNotEqual(old, fresh)
            try acknowledge(old, root: root)
            session.poll()
            XCTAssertFalse(FileManager.default.fileExists(atPath: input.path))
            try acknowledge(fresh, root: root)
            session.poll()
            XCTAssertEqual(try String(contentsOf: input, encoding: .utf8), "KEY ctrl+v\n")
        }
    }

    func testDisplayTeardownAndSessionReplacementCancelLegacyPaste() throws {
        for replace in [false, true] {
            try withSession { session, root in
                let view = FBLayerView(session: session)
                defer { view.teardown() }
                XCTAssertEqual(session.sendText("이전 화면"), .acceptedForProcessing)
                let old = try lastCommand(session)
                if replace {
                    let next = HvfEngineSession(config: session.config, repoRoot: root) { _ in false }
                    view.updateSession(next)
                } else {
                    view.teardown()
                }
                try acknowledge(old, root: root)
                session.poll()
                XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
            }
        }
    }

    private func withSession(_ body: (HvfEngineSession, URL) throws -> Void) throws {
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

    private func lastCommand(_ session: HvfEngineSession) throws -> String {
        try XCTUnwrap(String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
            .split(separator: "\n").last.map(String.init))
    }

    private func acknowledge(_ command: String, root: URL) throws {
        let range = try XCTUnwrap(command.range(of: "BVPASTE_READY [0-9A-Fa-f-]{36}", options: .regularExpression))
        let text = "BVAGENT CMD \(command) exit=0\n\(command[range])\nBVAGENT END \(command)\n"
        let handle = try FileHandle(forWritingTo: root.appendingPathComponent("run.log"))
        defer { try? handle.close() }
        try handle.seekToEnd(); try handle.write(contentsOf: Data(text.utf8))
    }
}
#endif
