import XCTest
import SwiftUI
@testable import BridgeVMControl

final class HvfHostShortcutIsolationTests: XCTestCase {
    func testCommandNeverBecomesAnUnmodifiedGuestKey() {
        let keys = ["\u{1b}", "\u{7f}", "\u{f728}", "\u{f700}", "\u{f701}",
                    "\u{f702}", "\u{f703}", "\u{f729}", "\u{f72b}",
                    "\u{f72c}", "\u{f72d}", "\t", "\r", "\n",
                    "\u{f704}", "\u{f70f}", "a", ""]
        let combinations: [EventModifiers] = [
            .command, [.command, .shift], [.command, .control, .option]
        ]
        for key in keys {
            for modifiers in combinations {
                XCTAssertEqual(HvfHostKeyCommand.resolve(
                    characters: key, modifiers: modifiers
                ), .ignored)
            }
        }
    }

    func testSecureAttentionStillRequiresNoHostCommandModifier() {
        XCTAssertEqual(HvfHostKeyCommand.resolve(
            characters: "\u{7f}", modifiers: [.control, .option]
        ), .key("ctrl+alt+delete"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(
            characters: "\u{f702}", modifiers: []
        ), .key("left"))
    }
}
