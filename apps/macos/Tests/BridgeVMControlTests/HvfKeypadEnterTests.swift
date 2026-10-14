import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfKeypadEnterTests: XCTestCase {
    func testKeypadEnterUsesExistingEnterAndAltEnterActions() {
        let keypad = String(UnicodeScalar(NSEnterCharacter)!)
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: keypad), .key("enter"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: keypad, modifiers: .option), .key("alt+enter"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: keypad, modifiers: .control), .key("ctrl+c"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: keypad, modifiers: .command), .ignored)
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: keypad, modifiers: .shift), .key("shift+enter"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "\r"), .key("enter"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "\n"), .key("enter"))
    }

    func testAppKitKeypadEventsReachLegacyHIDWithoutClipboard() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("keypad-enter-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
            evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        for flags: NSEvent.ModifierFlags in [.numericPad, [.numericPad, .option]] {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero,
                modifierFlags: flags, timestamp: 0, windowNumber: 0, context: nil,
                characters: "\u{3}", charactersIgnoringModifiers: "\u{3}", isARepeat: false, keyCode: 0x4c))
            XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session))
        }
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")),
                       "KEY enter\nKEY alt+enter\n")
        XCTAssertFalse(session.events.contains { $0.displayText.contains("clipboard paste") })
        XCTAssertFalse(FileManager.default.fileExists(atPath: config.ctlFilePath))
    }

    func testResolvedKeypadEnterUsesKeyProtocolRatherThanUnicodeControlText() throws {
        for modifiers: EventModifiers in [[], .option] {
            guard case let .key(key) = HvfHostKeyCommand.resolve(characters: "\u{3}", modifiers: modifiers) else {
                return XCTFail("Keypad Enter must resolve to a named key")
            }
            let encoding = try XCTUnwrap(HvfGuestInputEncoding(.key(key)))
            XCTAssertEqual(encoding.verb, "KEYINPUT")
            XCTAssertEqual(Data(base64Encoded: encoding.base64), Data(key.utf8))
            XCTAssertEqual(encoding.insertedEventCount, modifiers.isEmpty ? 2 : 4)
        }
    }
}
