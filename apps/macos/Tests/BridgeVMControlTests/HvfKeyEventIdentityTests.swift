import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfKeyEventIdentityTests: XCTestCase {
    func testControlKeypadEnterNeverBecomesCopyInLegacyDisplay() throws {
        try withDisplay { view, _, fixture in
            for flags: NSEvent.ModifierFlags in [.control, [.control, .numericPad]] {
                view.keyDown(with: try keypad(flags))
                XCTAssertNil(try fixture.bytes(fixture.input), "Unsupported Ctrl+Enter must not become Ctrl+C")
            }
            view.keyDown(with: try copy())
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY ctrl+c\n".utf8))
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

    func testControlKeypadEnterCancelsPreeditWithoutSendingTextOrCopy() throws {
        try withDisplay { view, _, fixture in
            view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0))
            XCTAssertTrue(view.hasMarkedText())
            view.keyDown(with: try keypad([.control, .numericPad]))
            XCTAssertFalse(view.hasMarkedText())
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    func testPlainOptionAndShiftKeypadRetainTheirExistingChords() throws {
        try withDisplay { view, _, fixture in
            for flags: NSEvent.ModifierFlags in [.numericPad, [.numericPad, .option], [.numericPad, .shift]] {
                view.keyDown(with: try keypad(flags))
            }
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY enter\nKEY alt+enter\nKEY shift+enter\n".utf8))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    func testCommandKeypadRemainsAHostShortcut() throws {
        try withDisplay { view, _, fixture in
            view.keyDown(with: try keypad([.command, .numericPad]))
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    func testAdditionalControlKeypadModifiersNeverBecomeGuestShortcuts() throws {
        try withDisplay { view, _, fixture in
            for flags: NSEvent.ModifierFlags in [[.control, .shift], [.control, .option], [.control, .command]] {
                view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
                    replacementRange: NSRange(location: NSNotFound, length: 0))
                view.keyDown(with: try keypad(flags))
                XCTAssertFalse(view.hasMarkedText())
            }
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
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
