import AppKit
import XCTest
import SwiftUI
@testable import BridgeVMControl

final class HvfShiftKeyChordTests: XCTestCase {
    func testShiftEnterAndSpaceReachGuestAsChords() throws {
        for (character, name) in [("\r", "enter"), ("\n", "enter"), ("\u{3}", "enter"), (" ", "space")] {
            let command = "shift+" + name
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: character, modifiers: .shift), .key(command))
            let request = try XCTUnwrap(HvfUnicodeInputRequest(event: .key(command), now: .init()))
            XCTAssertTrue(request.command.contains(Data(command.utf8).base64EncodedString()))
            XCTAssertEqual(request.insertedEventCount, 4)
        }
    }

    @MainActor
    func testLiveDisplayWritesExactShiftChordsWithoutClipboard() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("shift-input-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
            evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        for (character, code): (String, UInt16) in [("\r", 0x24), (" ", 0x31)] {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero,
                modifierFlags: .shift, timestamp: 0, windowNumber: 0, context: nil,
                characters: character, charactersIgnoringModifiers: character, isARepeat: false, keyCode: code))
            XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session))
        }
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")),
                       "KEY shift+enter\nKEY shift+space\n")
        XCTAssertFalse(FileManager.default.fileExists(atPath: config.ctlFilePath))
    }
}
