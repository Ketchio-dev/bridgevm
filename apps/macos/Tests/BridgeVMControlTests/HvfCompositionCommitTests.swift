#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// A chord, a bare modifier tap or a click first commits the syllable being composed, as a Mac does:
/// 감사합니다 then ⌘S saves 감사합니다, not 감사합니. Target changes and focus loss still discard it.
@MainActor
final class HvfCompositionCommitTests: XCTestCase {
    /// agent.ctl lines up to the end of negotiation; `deliver` acknowledges everything after them.
    private var negotiated = 0

    func testCommandChordSendsTheComposedSyllableBeforeTheChord() throws {
        try withOrderedDisplay { view, session, fixture in
            type(view, committed: "감사합니", composing: "다")
            XCTAssertTrue(view.performKeyEquivalent(with: try key("s", code: 0x01, flags: .command)))
            XCTAssertFalse(view.hasMarkedText())
            XCTAssertEqual(try deliver(fixture, session), ["TEXTINPUT 감사합니", "TEXTINPUT 다", "KEYINPUT ctrl+s"])
        }
    }

    func testControlAndOptionChordsAndABareCommandTapCommitFirst() throws {
        try withOrderedDisplay { view, session, fixture in
            type(view, committed: nil, composing: "한")
            view.keyDown(with: try key("\u{1}", ignoring: "a", code: 0x00, flags: .control))
            type(view, committed: nil, composing: "글")
            view.keyDown(with: try key("\u{f702}", code: 0x7b, flags: [.option, .numericPad, .function]))
            type(view, committed: nil, composing: "다")
            view.flagsChanged(with: try modifier(0x37, flags: .command))
            view.flagsChanged(with: try modifier(0x37, flags: []))
            XCTAssertFalse(view.hasMarkedText())
            XCTAssertEqual(try deliver(fixture, session), ["TEXTINPUT 한", "KEYINPUT ctrl+a", "TEXTINPUT 글",
                                                           "KEYINPUT ctrl+left", "TEXTINPUT 다", "KEYINPUT win"])
        }
    }

    func testClickOnTheOrderedRouteSendsTheSyllableBeforeThePress() throws {
        try withOrderedDisplay { view, session, fixture in
            type(view, committed: nil, composing: "다")
            view.mouseDown(with: try click())
            XCTAssertFalse(view.hasMarkedText())
            let sent = try deliver(fixture, session)
            XCTAssertEqual(sent.first, "TEXTINPUT 다")
            XCTAssertEqual(sent.dropFirst().first?.hasPrefix("POINTERINPUT press:"), true, "\(sent)")
        }
    }

    /// The older resident agent's route: Hangul is a clipboard paste. A chord waits behind it, but a
    /// press cancels a pending paste (#391), so a click still discards the syllable.
    func testClipboardRoutePastesTheSyllableBeforeTheChordAndAClickDiscardsIt() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try withDisplay(fixture) { view, session in
            try fixture.connect(session)
            type(view, committed: nil, composing: "다")
            XCTAssertTrue(view.performKeyEquivalent(with: try key("s", code: 0x01, flags: .command)))
            let paste = try XCTUnwrap(try fixture.commands().last)
            XCTAssertTrue(paste.contains(Data("다".utf8).base64EncodedString()), paste)
            XCTAssertNil(try fixture.bytes(fixture.input), "Ctrl+S waits behind the paste")
            try acknowledgePaste(paste, fixture)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY ctrl+v\nKEY ctrl+s\n".utf8))

            type(view, committed: nil, composing: "한")
            view.mouseDown(with: try click())
            XCTAssertFalse(view.hasMarkedText())
            XCTAssertEqual(try fixture.commands(), [paste], "no paste the press would cancel")
            let input = String(decoding: try XCTUnwrap(try fixture.bytes(fixture.input)), as: UTF8.self)
            XCTAssertTrue(input.hasPrefix("KEY ctrl+v\nKEY ctrl+s\nPOINTER press:"), input)
        }
    }

    func testTargetChangeAndFocusLossStillDiscard() throws {
        try withOrderedDisplay { view, session, fixture in
            type(view, committed: nil, composing: "다")
            view.windowResignedKey()
            XCTAssertFalse(view.hasMarkedText())
            type(view, committed: nil, composing: "한")
            session.cancelOrderedInputTarget()
            view.refreshFrame(at: 1)
            XCTAssertFalse(view.hasMarkedText())
            session.poll()
            XCTAssertFalse(try fixture.commands().contains { $0.hasPrefix("TEXTINPUT ") })
        }
    }

    // MARK: - Helpers

    private func type(_ view: FBLayerView, committed: String?, composing: String) {
        if let committed { view.insertText(committed, replacementRange: NSRange(location: NSNotFound, length: 0)) }
        view.setMarkedText(composing, selectedRange: NSRange(location: composing.utf16.count, length: 0),
                           replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertTrue(view.hasMarkedText())
    }

    /// Acknowledges each ordered command as the agent would and returns them as `VERB value`.
    private func deliver(_ fixture: HvfRuntimeKeyboardDraftFixture, _ session: HvfEngineSession) throws -> [String] {
        var sent: [String] = []
        var seen = negotiated
        while true {
            session.poll()
            session.poll()
            let commands = try fixture.commands()
            guard commands.count > seen else { return sent }
            for command in commands[seen...] {
                let words = command.split(separator: " ").map(String.init)
                guard words.count == 3, let data = Data(base64Encoded: words[2]) else { XCTFail(command); return sent }
                let value = String(decoding: data, as: UTF8.self)
                let event: HvfOrderedInputQueue.Event = switch words[0] {
                case "TEXTINPUT": .text(value)
                case "KEYINPUT": .key(value)
                default: .pointer(value)
                }
                let count = try XCTUnwrap(HvfGuestInputEncoding(event)?.insertedEventCount, command)
                sent.append(words[0] + " " + value)
                let label = words[0] + " " + words[1]
                try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(words[1]) \(count)\nBVAGENT END \(label)\n")
            }
            seen = commands.count
        }
    }

    private func acknowledgePaste(_ command: String, _ fixture: HvfRuntimeKeyboardDraftFixture) throws {
        let range = try XCTUnwrap(command.range(of: "BVPASTE_READY [0-9A-Fa-f-]{36}", options: .regularExpression))
        try fixture.appendLog("BVAGENT CMD \(command) exit=0\n\(command[range])\nBVAGENT END \(command)\n")
    }

    private func key(_ characters: String, ignoring: String? = nil, code: UInt16,
                     flags: NSEvent.ModifierFlags) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0,
            windowNumber: 0, context: nil, characters: characters, charactersIgnoringModifiers: ignoring ?? characters,
            isARepeat: false, keyCode: code))
    }

    private func modifier(_ code: UInt16, flags: NSEvent.ModifierFlags) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .flagsChanged, location: .zero, modifierFlags: flags, timestamp: 0,
            windowNumber: 0, context: nil, characters: "", charactersIgnoringModifiers: "", isARepeat: false,
            keyCode: code))
    }

    private func click() throws -> NSEvent {
        try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: NSPoint(x: 100, y: 50), modifierFlags: [],
            timestamp: 0, windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
    }

    private func withOrderedDisplay(
        _ body: (FBLayerView, HvfEngineSession, HvfRuntimeKeyboardDraftFixture) throws -> Void
    ) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try withDisplay(fixture) { view, session in
            try fixture.negotiateOrderedInput(session, modifierChords: true)
            XCTAssertTrue(view.guestModifierChords)
            negotiated = try fixture.commands().count
            try body(view, session, fixture)
        }
    }

    /// A display in a window with the view as first responder, as key equivalents need.
    private func withDisplay(_ fixture: HvfRuntimeKeyboardDraftFixture,
                             _ body: (FBLayerView, HvfEngineSession) throws -> Void) throws {
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
            let window = NSWindow(contentRect: view.frame, styleMask: [], backing: .buffered, defer: true)
            window.contentView = view
            defer { window.contentView = nil }
            XCTAssertTrue(window.makeFirstResponder(view))
            view.refreshFrame(at: 0)
            XCTAssertTrue(view.guestInputAvailable)
            try body(view, session)
        }
    }
}
#endif
