import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfOrderedSessionTransportTests: XCTestCase {
    @MainActor
    private func withSession(_ body: (HvfEngineSession, URL, URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("agent.ctl")
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true, ctlFilePath: control.path)
        try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(
            to: root.appendingPathComponent("run.log"), atomically: true, encoding: .utf8)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        XCTAssertTrue(session.attachToRunningVM())
        // Synthetic owned-boot boundary, not a real VM or Windows receipt.
        session.beginOwnedInputBoot()
        session.poll()
        let command = try commands(control).last ?? "missing"
        let id = command.split(separator: " ").last ?? "missing"
        try append("BVAGENT CMD \(command) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\nBVAGENT END \(command)\n", to: root)
        session.poll(); session.poll()
        try body(session, root, control)
    }

    private func commands(_ control: URL) throws -> [String] {
        try String(contentsOf: control, encoding: .utf8).split(separator: "\n").map(String.init)
    }

    private func append(_ value: String, to root: URL) throws {
        let handle = try FileHandle(forWritingTo: root.appendingPathComponent("run.log"))
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(value.utf8))
    }

    private func receipt(_ command: String, count: Int) -> String {
        let fields = command.split(separator: " ")
        let label = fields.prefix(2).joined(separator: " ")
        return "BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(fields[1]) \(count)\nBVAGENT END \(label)\n"
    }

    @MainActor
    func testSessionTextThenKeyUsesOneOrderedTransportWithoutHID() throws {
        try withSession { session, root, control in
            session.sendText("a")
            session.sendKey("enter")
            let before = try commands(control)
            XCTAssertEqual(before.count, 2)
            XCTAssertTrue(before[1].hasPrefix("TEXTINPUT "))
            try append(receipt(before[1], count: 2), to: root)
            session.poll()
            let after = try commands(control)
            XCTAssertEqual(after.count, 3)
            XCTAssertTrue(after[2].hasPrefix("KEYINPUT "))
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
        }
    }

    @MainActor
    func testFullSizedTextUsesBoundedInputTransportWithoutRelaxingGeneralCommands() throws {
        try withSession { session, root, control in
            session.sendText(String(repeating: "a", count: 65_536))
            let sent = try commands(control)
            XCTAssertEqual(sent.count, 2)
            XCTAssertTrue(sent[1].hasPrefix("TEXTINPUT "))
            XCTAssertGreaterThan(sent[1].utf8.count, HvfGuestCommand.maximumBytes)
            XCTAssertFalse(session.sendCtl(String(repeating: "x", count: 65_537)))
            XCTAssertEqual(try commands(control).count, 2)
        }
    }

    @MainActor
    func testTargetCancellationDiscardsUnsentTextWithoutLegacyFallback() throws {
        try withSession { session, root, control in
            session.sendText("first")
            session.sendText("discard")
            session.cancelOrderedInputTarget()
            session.sendKey("enter")
            let sent = try commands(control)
            XCTAssertEqual(sent.filter { $0.hasPrefix("TEXTINPUT ") }.count, 1)
            XCTAssertFalse(sent.contains { $0.hasPrefix("KEYINPUT ") })
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
        }
    }
}
