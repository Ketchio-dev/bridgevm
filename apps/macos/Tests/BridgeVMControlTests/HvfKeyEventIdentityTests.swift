import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfKeyEventIdentityTests: XCTestCase {
    func testControlKeypadEnterNeverBecomesCopyInLegacyDisplay() throws {
        try withDisplay { view, _, fixture in
            let enter = "KEY down:ctrl+enter\nKEY up:ctrl+enter\n"
            for (count, flags) in [(1, .control), (2, [.control, .numericPad])] as [(Int, NSEvent.ModifierFlags)] {
                try press(view, keypad(flags))
                XCTAssertEqual(try fixture.bytes(fixture.input), Data(String(repeating: enter, count: count).utf8), "Ctrl+keypad Enter is Ctrl+Enter, never Ctrl+C")
            }
            view.keyDown(with: try copy())
            XCTAssertEqual(try fixture.bytes(fixture.input), Data((enter + enter + "KEY down:ctrl+c\n").utf8))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    func testControlKeypadEnterNeverQueuesCopyOnNegotiatedDisplay() throws {
        try withDisplay { view, session, fixture in
            try fixture.negotiateOrderedInput(session)
            let before = try fixture.commands()
            view.keyDown(with: try keypad([.control, .numericPad]))
            XCTAssertEqual(try fixture.commands(), before)
            view.keyDown(with: try copy())
            let commands = try fixture.commands()
            XCTAssertEqual(commands.count, before.count + 1)
            let command = try XCTUnwrap(commands.last)
            XCTAssertTrue(command.hasPrefix("KEYINPUT "))
            XCTAssertEqual(command.split(separator: " ").last.map(String.init), Data("ctrl+c".utf8).base64EncodedString())
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testControlKeypadEnterEndsPreeditAndNeverSendsCopy() throws {
        try withDisplay { view, _, fixture in
            view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0))
            XCTAssertTrue(view.hasMarkedText())
            view.keyDown(with: try keypad([.control, .numericPad]))
            XCTAssertFalse(view.hasMarkedText())
            // The preedit is committed first, but Hangul needs the guest service, which is not connected here.
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY down:ctrl+enter\n".utf8), "never text or Ctrl+C")
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    func testPlainOptionAndShiftKeypadRetainTheirExistingChords() throws {
        try withDisplay { view, _, fixture in
            for flags: NSEvent.ModifierFlags in [.numericPad, [.numericPad, .option], [.numericPad, .shift]] {
                try press(view, keypad(flags))
            }
            XCTAssertEqual(try fixture.bytes(fixture.input), Data(["enter", "alt+enter", "shift+enter"]
                .map { "KEY down:\($0)\nKEY up:\($0)\n" }.joined().utf8))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    /// The VMM takes generic chords, so Command keypad Enter is the Mac-like Ctrl+Enter: Enter by key code, never Copy.
    func testCommandKeypadIsCtrlEnterOnTheLegacyDisplay() throws {
        try withDisplay { view, _, fixture in
            view.keyDown(with: try keypad([.command, .numericPad]))
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY ctrl+enter\n".utf8))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    func testAdditionalKeypadModifiersAreEnterChordsAndNeverCopy() throws {
        try withDisplay { view, _, fixture in
            for flags: NSEvent.ModifierFlags in [[.control, .shift], [.control, .option], [.control, .command]] {
                view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
                    replacementRange: NSRange(location: NSNotFound, length: 0))
                try press(view, keypad(flags))
                XCTAssertFalse(view.hasMarkedText())
            }
            XCTAssertEqual(try fixture.bytes(fixture.input), Data(("KEY down:ctrl+shift+enter\nKEY up:ctrl+shift+enter\n"
                + "KEY down:ctrl+alt+enter\nKEY up:ctrl+alt+enter\nKEY ctrl+enter\n").utf8), "a Command chord stays a tap")
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    /// An agent without MODIFIER_CHORDS keeps the earlier contract: these never become guest shortcuts.
    func testOlderAgentNeverQueuesCommandOrExtraKeypadModifiers() throws {
        try withDisplay { view, session, fixture in
            try fixture.negotiateOrderedInput(session)
            let before = try fixture.commands()
            for flags: NSEvent.ModifierFlags in [[.command, .numericPad], [.control, .shift], [.control, .option], [.control, .command]] {
                view.keyDown(with: try keypad(flags))
            }
            XCTAssertEqual(try fixture.commands(), before)
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    /// Its key-down, then its key-up: a key is held between them (HvfKeyHoldTests).
    private func press(_ view: FBLayerView, _ down: NSEvent) throws {
        view.keyDown(with: down)
        view.keyUp(with: try XCTUnwrap(NSEvent.keyEvent(with: .keyUp, location: .zero, modifierFlags: down.modifierFlags,
            timestamp: 0, windowNumber: 0, context: nil, characters: down.characters ?? "",
            charactersIgnoringModifiers: down.charactersIgnoringModifiers ?? "", isARepeat: false, keyCode: down.keyCode)))
    }

    private func keypad(_ flags: NSEvent.ModifierFlags) throws -> NSEvent {
        try event("\u{3}", ignoringModifiers: "\u{3}", code: 0x4c, flags: flags)
    }

    private func copy() throws -> NSEvent {
        try event("\u{3}", ignoringModifiers: "c", code: 0x08, flags: .control)
    }

    private func event(_ characters: String, ignoringModifiers: String, code: UInt16,
                       flags: NSEvent.ModifierFlags) throws -> NSEvent {
        let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags,
            timestamp: 0, windowNumber: 0, context: nil, characters: characters,
            charactersIgnoringModifiers: ignoringModifiers, isARepeat: false, keyCode: code))
        XCTAssertEqual(event.keyCode, code)
        XCTAssertEqual(event.characters, characters)
        XCTAssertEqual(event.charactersIgnoringModifiers, ignoringModifiers)
        XCTAssertEqual(event.modifierFlags.intersection(.deviceIndependentFlagsMask), flags)
        return event
    }

    private func withDisplay(_ body: (FBLayerView, HvfEngineSession, HvfRuntimeKeyboardDraftFixture) throws -> Void) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        var frame = Data(count: 68)
        for (offset, value): (Int, UInt32) in [(0, 0x42564642), (8, 1), (12, 1), (16, 4), (24, 2)] {
            var little = value.littleEndian
            withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
        }
        try frame.write(to: fixture.evidence.appendingPathComponent("display.fb"))
        try fixture.withSession { session in
            let view = FBLayerView(session: session)
            defer { view.teardown() }
            view.frame = NSRect(x: 0, y: 0, width: 320, height: 200)
            view.refreshFrame(at: 0)
            XCTAssertTrue(view.guestInputAvailable)
            try body(view, session, fixture)
        }
    }
}
