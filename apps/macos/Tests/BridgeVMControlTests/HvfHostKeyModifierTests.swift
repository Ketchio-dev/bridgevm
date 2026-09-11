import XCTest
import SwiftUI
@testable import BridgeVMControl

final class HvfHostKeyModifierTests: XCTestCase {
    func testNavigationAndEditingRetainGuestModifiers() {
        let cases: [(String, EventModifiers, String)] = [
            ("\u{f702}", .shift, "shift+left"), ("\u{f703}", .control, "ctrl+right"),
            ("\u{f729}", [.control, .shift], "ctrl+shift+home"),
            ("\t", .shift, "shift+tab"), ("\u{19}", .shift, "shift+tab"),
            ("\u{7f}", .control, "ctrl+backspace"), ("\u{f728}", .control, "ctrl+delete"),
            ("\u{f707}", .option, "alt+f4"), ("\t", .option, "alt+tab"),
            ("\r", .option, "alt+enter")
        ]
        for (characters, modifiers, key) in cases {
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: characters, modifiers: modifiers), .key(key))
        }
    }

    func testControlLettersAcceptPrintableAndControlCharacterForms() {
        for (letter, code) in [("a", 1), ("c", 3), ("v", 22), ("x", 24), ("y", 25), ("z", 26)] {
            let control = String(UnicodeScalar(UInt8(code)))
            for characters in [letter, letter.uppercased(), control] {
                XCTAssertEqual(HvfHostKeyCommand.resolve(
                    characters: characters, modifiers: .control
                ), .key("ctrl+" + letter))
            }
        }
    }

    func testUnsupportedModifiersNeverBecomePlainDestructiveKeys() {
        for (characters, modifiers): (String, EventModifiers) in [
            ("\u{f728}", .shift), ("\u{f702}", .option),
            ("\u{f703}", [.control, .option]), ("c", [.control, .shift])
        ] {
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: characters, modifiers: modifiers), .ignored)
        }
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "\u{f702}", modifiers: [.command, .shift]), .ignored)
    }

    func testShiftedTextAndSecureAttentionRemainAvailable() {
        for text in ["A", "!", " "] {
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: text, modifiers: .shift), .text(text))
        }
        XCTAssertEqual(HvfHostKeyCommand.resolve(
            characters: "\u{7f}", modifiers: [.control, .option]
        ), .key("ctrl+alt+delete"))
    }
}
