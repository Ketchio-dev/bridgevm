#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// Shift+click, Ctrl+click, Ctrl+scroll and the middle button through the real display view: an agent that
/// advertised MODIFIERS gets the held modifiers on each button and scroll request, and one that advertised
/// MIDDLE gets the middle button. Without the markers nothing changes: modifiers dropped, middle left to the Mac.
@MainActor
final class HvfPointerModifierInputTests: XCTestCase {
    /// A synthetic AppKit button or scroll event; the view reads only these fields.
    private final class PointerEvent: NSEvent {
        private let flags: NSEvent.ModifierFlags, button: Int, location: NSPoint
        init(_ flags: NSEvent.ModifierFlags = [], button: Int = 0, at location: NSPoint = NSPoint(x: 160, y: 100)) {
            self.flags = flags; self.button = button; self.location = location
            super.init()
        }
        required init?(coder: NSCoder) { nil }
        override var modifierFlags: NSEvent.ModifierFlags { flags }
        override var buttonNumber: Int { button }
        override var locationInWindow: NSPoint { location }
        override var scrollingDeltaX: CGFloat { 0 }
        override var scrollingDeltaY: CGFloat { 1 }
        override var hasPreciseScrollingDeltas: Bool { false }
        override var phase: NSEvent.Phase { [] }
        override var momentumPhase: NSEvent.Phase { [] }
    }

    /// agent.ctl lines up to the end of negotiation; `deliver` acknowledges everything after them.
    private var negotiated = 0

    /// Before this change the guest got a plain press and release: Shift+click could not extend a selection.
    func testAShiftClickReachesTheGuestWithShiftHeldOnItsPressAndRelease() throws {
        try withOrderedDisplay(pointer: ["MODIFIERS"]) { view, session, fixture in
            view.mouseDown(with: PointerEvent(.shift))
            view.mouseUp(with: PointerEvent(.shift))
            view.mouseDown(with: PointerEvent())
            view.mouseUp(with: PointerEvent())
            XCTAssertEqual(try deliver(fixture, session), ["press:16384x16384:s", "releaseall:16384x16384:s",
                                                           "press:16384x16384", "releaseall:16384x16384"])
        }
    }

    func testCommandClickIsCtrlClickOnTheMacStyleAndWinClickWithTheWindowsKeyStyle() throws {
        try withOrderedDisplay(pointer: ["MODIFIERS"]) { view, session, fixture in
            view.mouseDown(with: PointerEvent([.command, .option, .capsLock]))
            view.mouseUp(with: PointerEvent([.control, .shift, .function]))
            view.rightMouseDown(with: PointerEvent(.command))
            view.rightMouseUp(with: PointerEvent())
            session.config.commandKeyMode = .windows
            view.mouseDown(with: PointerEvent([.command, .shift]))
            view.mouseUp(with: PointerEvent(.capsLock))
            XCTAssertEqual(try deliver(fixture, session), ["press:16384x16384:ca", "releaseall:16384x16384:cs",
                                                           "rightpress:16384x16384:c", "releaseall:16384x16384",
                                                           "press:16384x16384:ws", "releaseall:16384x16384"])
        }
    }

    /// A drag's press and release carry what is held at each; its moves carry nothing, so the guest gets no
    /// modifier key per frame.
    func testADragCarriesModifiersOnItsPressAndReleaseOnly() throws {
        try withOrderedDisplay(pointer: ["MODIFIERS"]) { view, session, fixture in
            view.mouseDown(with: PointerEvent(.option))
            view.mouseDragged(with: PointerEvent(.option, at: NSPoint(x: 80, y: 50)))
            view.refreshFrame(at: 1)
            view.mouseUp(with: PointerEvent(.option))
            let sent = try deliver(fixture, session)
            XCTAssertEqual(sent.count, 3, "\(sent)")
            XCTAssertEqual(sent.first, "press:16384x16384:a")
            XCTAssertEqual(sent.dropFirst().first?.hasPrefix("move:"), true, "\(sent)")
            XCTAssertEqual(sent.dropFirst().first?.contains(":a"), false, "\(sent)")
            XCTAssertEqual(sent.last, "releaseall:16384x16384:a")
        }
    }

    func testCtrlScrollCarriesCtrlAsUnitsOrNotches() throws {
        try withOrderedDisplay(pointer: ["SCROLL_UNITS", "MODIFIERS"]) { view, session, fixture in
            view.scrollWheel(with: PointerEvent(.command))
            XCTAssertEqual(try deliver(fixture, session), ["scrollunits:120,0@16384x16384:c"])
        }
        try withOrderedDisplay(pointer: ["MODIFIERS"]) { view, session, fixture in
            view.scrollWheel(with: PointerEvent(.shift))
            XCTAssertEqual(try deliver(fixture, session), ["scroll:1@16384x16384:s"])
        }
    }

    func testAMiddleClickAndDragReachOnlyAnAgentThatTakesTheMiddleButton() throws {
        try withOrderedDisplay(pointer: ["MIDDLE"]) { view, session, fixture in
            view.otherMouseDown(with: PointerEvent(.command, button: 2))
            view.otherMouseDragged(with: PointerEvent(button: 2, at: NSPoint(x: 80, y: 50)))
            view.refreshFrame(at: 1)
            view.otherMouseUp(with: PointerEvent(button: 2))
            view.otherMouseDown(with: PointerEvent(button: 3))
            view.otherMouseUp(with: PointerEvent(button: 3))
            let sent = try deliver(fixture, session)
            XCTAssertEqual(sent.count, 3, "\(sent)")
            XCTAssertEqual(sent.first, "middlepress:16384x16384", "no MODIFIERS marker: Command is dropped")
            XCTAssertEqual(sent.dropFirst().first?.hasPrefix("move:"), true, "\(sent)")
            XCTAssertEqual(sent.last, "releaseall:16384x16384", "button 3 stays with the Mac")
        }
        try withOrderedDisplay(pointer: ["MIDDLE", "MODIFIERS"]) { view, session, fixture in
            view.otherMouseDown(with: PointerEvent(.command, button: 2))
            view.otherMouseUp(with: PointerEvent(.command, button: 2))
            XCTAssertEqual(try deliver(fixture, session), ["middlepress:16384x16384:c", "releaseall:16384x16384:c"])
        }
    }

    /// An agent without the markers, and the VMM's HID path, get exactly what they got before.
    func testWithoutTheMarkersModifiersAreDroppedAndTheMiddleButtonStaysWithTheMac() throws {
        try withOrderedDisplay(pointer: ["SCROLL_UNITS"]) { view, session, fixture in
            view.mouseDown(with: PointerEvent(.shift))
            view.mouseUp(with: PointerEvent(.shift))
            view.otherMouseDown(with: PointerEvent(button: 2))
            view.otherMouseUp(with: PointerEvent(button: 2))
            view.scrollWheel(with: PointerEvent(.command))
            XCTAssertEqual(try deliver(fixture, session), ["press:16384x16384", "releaseall:16384x16384",
                                                           "scrollunits:120,0@16384x16384"])
        }
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try withDisplay(fixture) { view, session in
            try fixture.connect(session)
            view.mouseDown(with: PointerEvent(.shift))
            view.mouseUp(with: PointerEvent(.shift))
            view.otherMouseDown(with: PointerEvent(button: 2))
            view.otherMouseUp(with: PointerEvent(button: 2))
            let input = try XCTUnwrap(try fixture.bytes(fixture.input))
            XCTAssertEqual(String(decoding: input, as: UTF8.self), "POINTER press:16384x16384\nPOINTER release:16384x16384\n")
        }
    }

    /// A Command held for a click is a modifier, not a bare tap: releasing it must not also open Start.
    func testAModifiedClickStillCancelsABareCommandTap() throws {
        try withOrderedDisplay(pointer: ["MODIFIERS"]) { view, session, fixture in
            view.flagsChanged(with: try modifier(0x37, flags: .command))
            view.mouseDown(with: PointerEvent(.command))
            view.mouseUp(with: PointerEvent(.command))
            view.flagsChanged(with: try modifier(0x37, flags: []))
            view.flagsChanged(with: try modifier(0x37, flags: .command))
            view.flagsChanged(with: try modifier(0x37, flags: []))
            XCTAssertEqual(try deliver(fixture, session), ["press:16384x16384:c", "releaseall:16384x16384:c", "KEYINPUT win"])
        }
    }

    private func modifier(_ code: UInt16, flags: NSEvent.ModifierFlags) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .flagsChanged, location: .zero, modifierFlags: flags, timestamp: 0,
            windowNumber: 0, context: nil, characters: "", charactersIgnoringModifiers: "", isARepeat: false,
            keyCode: code))
    }

    /// Acknowledges each ordered command as the agent would and returns pointer payloads, other input as `VERB value`.
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
                let event: HvfOrderedInputQueue.Event = words[0] == "KEYINPUT" ? .key(value) : .pointer(value)
                let count = try XCTUnwrap(HvfGuestInputEncoding(event)?.insertedEventCount, command)
                sent.append(words[0] == "POINTERINPUT" ? value : words[0] + " " + value)
                let label = words[0] + " " + words[1]
                try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(words[1]) \(count)\nBVAGENT END \(label)\n")
            }
            seen = commands.count
        }
    }

    private func withOrderedDisplay(
        pointer: [String], _ body: (FBLayerView, HvfEngineSession, HvfRuntimeKeyboardDraftFixture) throws -> Void
    ) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try withDisplay(fixture) { view, session in
            try fixture.negotiateOrderedInput(session, modifierChords: true, pointer: pointer)
            negotiated = try fixture.commands().count
            try body(view, session, fixture)
        }
    }

    /// A one-pixel guest display in a window, the view first responder.
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
