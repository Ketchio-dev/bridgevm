#if canImport(AppKit)
import AppKit
import XCTest
import SwiftUI
@testable import BridgeVMControl

@MainActor
final class HvfShiftKeyChordTests: XCTestCase {
    private let cases: [(String, UInt16, String)] = [
        ("\r", 0x24, "enter"), ("\n", 0x24, "enter"), ("\u{3}", 0x4c, "enter"), (" ", 0x31, "space")
    ]

    func testShiftEnterAndSpaceEncodeAsFourEventChords() throws {
        for (character, _, name) in cases {
            let command = "shift+" + name
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: character, modifiers: .shift), .key(command))
            let request = try XCTUnwrap(HvfUnicodeInputRequest(event: .key(command), now: .init()))
            XCTAssertTrue(request.command.contains(Data(command.utf8).base64EncodedString()))
            XCTAssertEqual(request.insertedEventCount, 4)
        }
    }

    func testDisplayKeyDownWritesExactShiftChordsToControlFile() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("shift-input-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        var frame = Data(count: 68)
        for (offset, value): (Int, UInt32) in [(0, 0x42564642), (8, 1), (12, 1), (16, 4), (24, 2)] {
            var little = value.littleEndian
            withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
        }
        try frame.write(to: root.appendingPathComponent("display.fb"))
        let config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
            evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let view = FBLayerView(session: session)
        defer { view.teardown() }
        view.frame = NSRect(x: 0, y: 0, width: 320, height: 200)
        view.refreshFrame(at: 0)
        XCTAssertTrue(view.guestInputAvailable)
        for (character, code, _) in cases { view.keyDown(with: try event(character, code: code)) }
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")),
                       "KEY shift+enter\nKEY shift+enter\nKEY shift+enter\nKEY shift+space\n")
        XCTAssertFalse(FileManager.default.fileExists(atPath: config.ctlFilePath))
    }

    func testShiftEnterAndSpaceRemainWithInputMethodWhileMarked() throws {
        let view = RecordingView()
        view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
        for (character, code, _) in cases { view.keyDown(with: try event(character, code: code)) }
        XCTAssertEqual(view.interpreted.compactMap(\.characters), cases.map { $0.0 })
        XCTAssertTrue(view.interpreted.allSatisfy { $0.modifierFlags.contains(.shift) })
        XCTAssertTrue(view.hasMarkedText())
        XCTAssertTrue(view.keys.isEmpty)
        XCTAssertTrue(view.texts.isEmpty)
    }

    private func event(_ character: String, code: UInt16) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .shift,
            timestamp: 0, windowNumber: 0, context: nil, characters: character,
            charactersIgnoringModifiers: character, isARepeat: false, keyCode: code))
    }

    private final class RecordingView: HvfFramebufferTextInput {
        var keys: [String] = [], texts: [String] = [], interpreted: [NSEvent] = []
        override var guestInputAvailable: Bool { true }
        override func sendGuestKeyEvent(_ event: NSEvent) -> Bool { keys.append(event.characters ?? ""); return true }
        override func sendGuestText(_ text: String) -> Bool { texts.append(text); return true }
        override func interpretKeyEvents(_ events: [NSEvent]) { interpreted += events }
    }
}
#endif
