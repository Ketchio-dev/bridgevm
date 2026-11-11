import AppKit
import XCTest
@testable import BridgeVMControl

/// A tapped key's repeats never pile up behind a slow ordered route; first presses always queue.
@MainActor
final class HvfDisplayKeyRepeatTests: XCTestCase {
    private func backspace(repeat isRepeat: Bool) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
            windowNumber: 0, context: nil, characters: "\u{7f}", charactersIgnoringModifiers: "\u{7f}",
            isARepeat: isRepeat, keyCode: 51))
    }

    func testRepeatsBehindAnUnsentTicketAreAbsorbedButNewPressesQueue() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            var refusals = 0
            XCTAssertTrue(HvfFramebufferKeyInput.send(try backspace(repeat: false), to: session) { refusals += 1 })
            XCTAssertEqual(session.inputDriver.router.count, 1)
            let sent = try fixture.commands()
            XCTAssertTrue(try XCTUnwrap(sent.last).hasPrefix("KEYINPUT "))
            for _ in 0..<20 {
                XCTAssertTrue(HvfFramebufferKeyInput.send(try backspace(repeat: true), to: session) { refusals += 1 })
            }
            XCTAssertEqual(session.inputDriver.router.count, 2, "one repeat waits behind the ticket in flight")
            XCTAssertTrue(HvfFramebufferKeyInput.send(try backspace(repeat: false), to: session) { refusals += 1 })
            XCTAssertEqual(session.inputDriver.router.count, 3, "a new press is never absorbed")
            XCTAssertEqual(refusals, 0)
            XCTAssertEqual(try fixture.commands(), sent, "absorption writes nothing to the guest")
        }
    }

    /// The VMM holds the key in its HID report and Windows repeats it (HvfKeyHoldTests), so no Mac repeat is written.
    func testLegacyRouteHoldsTheKeyAndWritesNoRepeat() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            XCTAssertTrue(HvfFramebufferKeyInput.send(try backspace(repeat: false), to: session))
            for _ in 0..<3 { XCTAssertTrue(HvfFramebufferKeyInput.send(try backspace(repeat: true), to: session)) }
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY down:backspace\n".utf8))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }
}
