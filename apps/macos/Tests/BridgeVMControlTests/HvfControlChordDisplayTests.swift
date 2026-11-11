import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfControlChordDisplayTests: XCTestCase {
    private let events: [(String, UInt16, NSEvent.ModifierFlags, String)] = [
        ("\u{13}", 0x01, .control, "ctrl+s"), ("ㄴ", 0x01, .control, "ctrl+s"), ("\u{17}", 0x0d, .control, "ctrl+w"),
        ("\r", 0x24, .control, "ctrl+enter"), ("\u{3}", 0x4c, [.control, .numericPad], "ctrl+enter"),
        ("\t", 0x30, .control, "ctrl+tab"), ("\u{19}", 0x30, [.control, .shift], "ctrl+shift+tab"),
        ("\u{f70d}", 0x6d, [.shift, .function], "shift+f10"), ("\u{f728}", 0x75, [.shift, .function], "shift+delete")
    ]

    /// The VMM shipped with the app takes generic chords, so Command is Ctrl there (the Mac-like default)
    /// and only the app shortcuts the Mac keeps stay local.
    func testLegacyDisplayEventsWriteChordsAsHidKeysAndCommandIsCtrlExceptMacShortcuts() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("control-chords-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
            evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        for (characters, code, flags, _) in events {
            XCTAssertTrue(HvfFramebufferKeyInput.send(try event(characters, code: code, flags: flags), to: session,
                                                      refused: { XCTFail("legacy refusal") }))
            XCTAssertTrue(HvfFramebufferKeyInput.send(try event(characters, code: code, flags: flags, type: .keyUp), to: session))
        }
        XCTAssertTrue(HvfFramebufferKeyInput.send(try event("s", code: 0x01, flags: .command), to: session))
        XCTAssertTrue(HvfFramebufferKeyInput.send(try event("\u{13}", code: 0x01, flags: [.command, .control]), to: session))
        XCTAssertFalse(HvfFramebufferKeyInput.send(try event("q", code: 0x0c, flags: .command), to: session))
        XCTAssertFalse(HvfFramebufferKeyInput.send(try event("f", code: 0x03, flags: [.command, .control]), to: session))
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")),
                       events.map { "KEY down:\($0.3)\nKEY up:\($0.3)\n" }.joined() + "KEY ctrl+s\nKEY ctrl+s\n",
                       "each chord is held from its key-down to its key-up; a Command chord stays a tap")
        XCTAssertFalse(FileManager.default.fileExists(atPath: config.ctlFilePath))
    }

    func testOldAgentDisplayChordsSignalRefusalWithoutAnyTransportWrite() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session) // base v3 marker only
            let before = try fixture.commands()
            var refusals = 0
            for (characters, code, flags, _) in events {
                XCTAssertTrue(HvfFramebufferKeyInput.send(try event(characters, code: code, flags: flags), to: session,
                                                          refused: { refusals += 1 }))
            }
            XCTAssertEqual(refusals, events.count)
            XCTAssertEqual(try fixture.commands(), before)
            XCTAssertNil(try fixture.bytes(fixture.input), "No cross-transport fallback")
            XCTAssertTrue(session.sendDisplayInput(.key("ctrl+c")), "existing chord still queues")
            XCTAssertTrue(try XCTUnwrap(fixture.commands().last).hasPrefix("KEYINPUT "))
        }
    }

    private func event(_ characters: String, code: UInt16, flags: NSEvent.ModifierFlags,
                       type: NSEvent.EventType = .keyDown) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: type, location: .zero, modifierFlags: flags, timestamp: 0,
            windowNumber: 0, context: nil, characters: characters, charactersIgnoringModifiers: characters,
            isARepeat: false, keyCode: code))
    }
}
