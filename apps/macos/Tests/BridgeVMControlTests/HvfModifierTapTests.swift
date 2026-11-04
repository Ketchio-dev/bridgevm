import AppKit
import XCTest
@testable import BridgeVMControl

final class HvfModifierTapTests: XCTestCase {
    private let command: UInt16 = 0x37, rightCommand: UInt16 = 0x36, option: UInt16 = 0x3a, shift: UInt16 = 0x38

    func testBareCommandIsTheWindowsKeyAndBareOptionIsAlt() {
        var tap = HvfModifierTap()
        XCTAssertNil(tap.changed(keyCode: command, flags: .command))
        XCTAssertEqual(tap.changed(keyCode: command, flags: []), "win")
        XCTAssertNil(tap.changed(keyCode: rightCommand, flags: [.command, .capsLock]), "Caps Lock is not a modifier here")
        XCTAssertEqual(tap.changed(keyCode: rightCommand, flags: .capsLock), "win")
        XCTAssertNil(tap.changed(keyCode: option, flags: .option))
        XCTAssertEqual(tap.changed(keyCode: option, flags: []), "alt")
        XCTAssertNil(tap.changed(keyCode: command, flags: []), "a release without its press is no tap")
    }

    func testAnotherModifierOrACancelEndsThePendingTap() {
        var tap = HvfModifierTap()
        XCTAssertNil(tap.changed(keyCode: command, flags: .command))
        XCTAssertNil(tap.changed(keyCode: shift, flags: [.command, .shift]))
        XCTAssertNil(tap.changed(keyCode: shift, flags: .command))
        XCTAssertNil(tap.changed(keyCode: command, flags: []), "Command+Shift released without a key")

        XCTAssertNil(tap.changed(keyCode: shift, flags: .shift))
        XCTAssertNil(tap.changed(keyCode: command, flags: [.shift, .command]), "pressed while another modifier was down")
        XCTAssertNil(tap.changed(keyCode: command, flags: .shift))

        // A key, click, scroll or focus loss (Command+Tab, window resign) cancels it.
        XCTAssertNil(tap.changed(keyCode: command, flags: .command))
        tap.cancel()
        XCTAssertNil(tap.changed(keyCode: command, flags: []))
        XCTAssertNil(tap.changed(keyCode: option, flags: .option))
        tap.cancel()
        XCTAssertNil(tap.changed(keyCode: option, flags: []))
    }

    func testOptionPressedDuringCommandIsNeitherTap() {
        var tap = HvfModifierTap()
        XCTAssertNil(tap.changed(keyCode: command, flags: .command))
        XCTAssertNil(tap.changed(keyCode: option, flags: [.command, .option]))
        XCTAssertNil(tap.changed(keyCode: option, flags: .command))
        XCTAssertNil(tap.changed(keyCode: command, flags: []))
    }
}
