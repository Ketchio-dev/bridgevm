import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfClipboardPasteSessionTests: XCTestCase {
    @MainActor
    func testPasteRequiresCompleteCorrelatedSuccessAndEmitsOnce() throws {
        try withSession { session, root, control in
            session.sendText("\u{d55c}\u{ae00}")
            let command = try String(contentsOf: control, encoding: .utf8)
                .trimmingCharacters(in: .newlines)
            let markerRange = try XCTUnwrap(command.range(
                of: "BVPASTE_READY [0-9A-Fa-f-]{36}", options: .regularExpression
            ))
            let marker = String(command[markerRange])
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
            session.sendText("must not overtake");
            XCTAssertEqual(try String(contentsOf: control, encoding: .utf8), command + "\n")
            try append("BVAGENT CMD \(command) exit=0\n", to: root)
            session.poll()
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
            try append(marker + "\n", to: root)
            session.poll()
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
            let end = "BVAGENT END \(command)\n"
            try append(end, to: root)
            session.poll()
            let input = root.appendingPathComponent("input.ctl")
            XCTAssertEqual(try String(contentsOf: input, encoding: .utf8), "KEY ctrl+v\n")
            try append("BVAGENT CMD \(command) exit=0\n" + marker + "\n" + end, to: root)
            session.poll()
            XCTAssertEqual(try String(contentsOf: input, encoding: .utf8), "KEY ctrl+v\n")
        }
    }

    @MainActor
    func testFailedCommandAndRestartNeverTriggerPaste() throws {
        for restart in ["none", "before", "after"] {
            try withSession { session, root, control in
                session.sendText("\u{d55c}\u{ae00}")
                let command = try String(contentsOf: control, encoding: .utf8)
                    .trimmingCharacters(in: .newlines)
                let markerRange = try XCTUnwrap(command.range(
                    of: "BVPASTE_READY [0-9A-Fa-f-]{36}", options: .regularExpression
                ))
                let prefix = restart == "before" ? "BVAGENT READY host=test t=3\n" : ""
                let exit = restart == "none" ? 1 : 0; let suffix = restart == "after" ? "BVAGENT READY host=test t=3\n" : ""
                try append(prefix + "BVAGENT CMD \(command) exit=\(exit)\n"
                    + String(command[markerRange]) + "\nBVAGENT END \(command)\n" + suffix, to: root)
                session.poll()
                XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
            }
        }
    }

    @MainActor
    private func withSession(
        _ body: (HvfEngineSession, URL, URL) throws -> Void
    ) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("agent.ctl")
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: control.path
        )
        try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(
            to: root.appendingPathComponent("run.log"), atomically: true, encoding: .utf8
        )
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        XCTAssertTrue(session.attachToRunningVM())
        try body(session, root, control)
    }

    private func append(_ text: String, to root: URL) throws {
        let handle = try FileHandle(forWritingTo: root.appendingPathComponent("run.log"))
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
