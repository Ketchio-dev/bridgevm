#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferFocusBindingTests: XCTestCase {
    /// The window's resignation reaches the replacement session: its held key goes up. Windows keeps its focus, so the
    /// text queued behind the request in flight still goes in first.
    func testSessionReplacementRebindsWindowResignation() async throws {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let first = try session(root.appendingPathComponent("first"))
        let secondRoot = root.appendingPathComponent("second")
        let second = try session(secondRoot)
        defer { first.beginOwnedInputBoot(); second.beginOwnedInputBoot() }
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let view = FBLayerView(session: first)
        window.contentView = view
        defer { view.teardown(); window.contentView = nil }
        view.updateSession(second)
        XCTAssertEqual(second.sendText("active"), .acceptedForProcessing)
        XCTAssertTrue(second.sendKeyDown("left", keyCode: 0x7b, repeat: false))
        XCTAssertEqual(second.sendText("kept"), .acceptedForProcessing)
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        for _ in 0..<10 { await Task.yield() }
        for count in [12, 1, 8] { try insert(count, second, root: secondRoot) }
        XCTAssertEqual(try payloads(second), ["TEXTINPUT active", "KEYINPUT down:left", "TEXTINPUT kept", "KEYINPUT up:left"])
        XCTAssertFalse(FileManager.default.fileExists(atPath:
            URL(fileURLWithPath: second.config.evidenceDir).appendingPathComponent("input.ctl").path))
    }

    private func session(_ root: URL) throws -> HvfEngineSession {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let result = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        try append("BVAGENT READY host=synthetic t=1\nBVAGENT SERVICE start t=2\n", root: root)
        result.poll(); result.beginOwnedInputBoot(); result.poll()
        let command = try XCTUnwrap(try commands(result).last)
        let id = try XCTUnwrap(command.split(separator: " ").last)
        try append("BVAGENT CMD \(command) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
            + "BVINPUT_KEYS \(id) HOLD\nBVAGENT END \(command)\n", root: root)
        result.poll(); result.poll()
        return result
    }

    private func commands(_ session: HvfEngineSession) throws -> [String] {
        try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
            .split(separator: "\n").map(String.init)
    }

    /// The agent's receipt for the request it got last.
    private func insert(_ count: Int, _ session: HvfEngineSession, root: URL) throws {
        let words = try XCTUnwrap(try commands(session).last).split(separator: " ")
        let label = words.prefix(2).joined(separator: " ")
        try append("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(words[1]) \(count)\nBVAGENT END \(label)\n", root: root)
        session.poll()
    }

    /// `VERB payload` of each input request, INPUTCAPS left out.
    private func payloads(_ session: HvfEngineSession) throws -> [String] {
        try commands(session).compactMap { command in
            let words = command.split(separator: " ")
            guard words.count == 3, let data = Data(base64Encoded: String(words[2])) else { return nil }
            return "\(words[0]) " + String(decoding: data, as: UTF8.self)
        }
    }

    private func append(_ text: String, root: URL) throws {
        let url = root.appendingPathComponent("run.log")
        if !FileManager.default.fileExists(atPath: url.path) { try Data().write(to: url) }
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.seekToEnd(); try handle.write(contentsOf: Data(text.utf8))
    }
}
#endif
