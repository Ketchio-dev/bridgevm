import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPasteKeyOrderingTests: XCTestCase {
    @MainActor
    func testDirectKeysCannotOvertakePendingPaste() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("agent.ctl")
        let input = root.appendingPathComponent("input.ctl")
        let log = root.appendingPathComponent("run.log")
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: control.path
        )
        try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(
            to: log, atomically: true, encoding: .utf8
        )
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        XCTAssertTrue(session.attachToRunningVM())
        session.sendText("\u{d55c}\u{ae00}")
        let command = try String(contentsOf: control, encoding: .utf8)
            .trimmingCharacters(in: .newlines)
        let markerRange = try XCTUnwrap(command.range(
            of: "BVPASTE_READY [0-9A-Fa-f-]{36}", options: .regularExpression
        ))
        session.sendKey("enter")
        session.sendKey("left")
        XCTAssertFalse(FileManager.default.fileExists(atPath: input.path))
        XCTAssertTrue(session.events.contains { event in
            if case let .unknown(message) = event {
                return message == "key input refused: clipboard paste pending"
            }
            return false
        })
        let handle = try FileHandle(forWritingTo: log)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(("BVAGENT CMD \(command) exit=0\n"
            + String(command[markerRange]) + "\nBVAGENT END \(command)\n").utf8))
        try handle.close()
        session.poll()
        session.sendKey("enter")
        XCTAssertEqual(try String(contentsOf: input, encoding: .utf8), "KEY ctrl+v\nKEY enter\n")
    }
}
