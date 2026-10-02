#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferFocusBindingTests: XCTestCase {
    func testSessionReplacementRebindsWindowResignationCancellation() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let first = try session(root.appendingPathComponent("first"))
        let second = try session(root.appendingPathComponent("second"))
        defer { first.beginOwnedInputBoot(); second.beginOwnedInputBoot() }
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let view = FBLayerView(session: first)
        window.contentView = view
        defer { view.teardown(); window.contentView = nil }
        view.updateSession(second)
        XCTAssertEqual(second.sendText("active"), .acceptedForProcessing)
        XCTAssertEqual(second.sendText("discard-on-resign"), .acceptedForProcessing)
        let before = try commands(second)
        XCTAssertEqual(before.filter { $0.hasPrefix("TEXTINPUT ") }.count, 1)
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        for _ in 0..<10 { await Task.yield() }
        second.sendKey("enter")
        let after = try commands(second)
        XCTAssertEqual(after.filter { $0.hasPrefix("KEYINPUT ") }.count, 1,
            "resignation must clear old queued input so a new target can send immediately")
        XCTAssertEqual(after.filter { $0.hasPrefix("TEXTINPUT ") }.count, 1)
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
        try append("BVAGENT CMD \(command) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\nBVAGENT END \(command)\n", root: root)
        result.poll(); result.poll()
        return result
    }

    private func commands(_ session: HvfEngineSession) throws -> [String] {
        try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
            .split(separator: "\n").map(String.init)
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
