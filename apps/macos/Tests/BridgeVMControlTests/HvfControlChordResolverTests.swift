import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfControlChordResolverTests: XCTestCase {
    /// Carbon kVK_ANSI_* positions, written out independently of the resolver's table.
    private let ansi: [(UInt16, String)] = [
        (0x00, "a"), (0x0b, "b"), (0x08, "c"), (0x02, "d"), (0x0e, "e"), (0x03, "f"), (0x05, "g"),
        (0x04, "h"), (0x22, "i"), (0x26, "j"), (0x28, "k"), (0x25, "l"), (0x2e, "m"), (0x2d, "n"),
        (0x1f, "o"), (0x23, "p"), (0x0c, "q"), (0x0f, "r"), (0x01, "s"), (0x11, "t"), (0x20, "u"),
        (0x09, "v"), (0x0d, "w"), (0x07, "x"), (0x10, "y"), (0x06, "z")
    ]

    func testEveryControlLetterResolvesFromLetterAndControlCharacterForms() {
        for (index, letter) in "abcdefghijklmnopqrstuvwxyz".map(String.init).enumerated() {
            let control = String(UnicodeScalar(UInt8(index + 1)))
            for characters in [letter, letter.uppercased(), control] {
                XCTAssertEqual(HvfHostKeyCommand.resolve(characters: characters, modifiers: .control),
                               .key("ctrl+" + letter), "\(characters.unicodeScalars.map(\.value))")
            }
        }
    }

    func testAppKitControlLetterEventsResolveAtEveryAnsiPosition() throws {
        XCTAssertEqual(Set(ansi.map { $0.1 }).count, 26)
        for (code, letter) in ansi {
            let control = String(UnicodeScalar(UInt8(letter.unicodeScalars.first!.value - 96)))
            for characters in [control, letter] {
                XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event(characters, code: code)), .key("ctrl+" + letter), letter)
            }
        }
    }

    func testPhysicalReturnKeypadEnterAndTabWinOverSharedControlCharacters() throws {
        let cases: [(String, UInt16, NSEvent.ModifierFlags, String)] = [
            ("\r", 0x24, .control, "ctrl+enter"), ("\n", 0x24, .control, "ctrl+enter"),
            ("\u{3}", 0x4c, [.control, .numericPad], "ctrl+enter"), ("\t", 0x30, .control, "ctrl+tab"),
            ("\u{19}", 0x30, [.control, .shift], "ctrl+shift+tab"), ("\t", 0x30, [.control, .shift], "ctrl+shift+tab"),
            ("\r", 0x2e, .control, "ctrl+m"), ("\t", 0x22, .control, "ctrl+i"), ("\n", 0x26, .control, "ctrl+j"),
            ("\u{8}", 0x04, .control, "ctrl+h"), ("\u{3}", 0x08, .control, "ctrl+c"), ("\u{19}", 0x10, .control, "ctrl+y")
        ]
        for (characters, code, flags, key) in cases {
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event(characters, code: code, flags: flags)), .key(key), key)
        }
    }

    func testShiftFunctionKeysAndShiftDeleteBecomeChords() throws {
        for number in 1...12 {
            let scalar = String(try XCTUnwrap(UnicodeScalar(UInt32(0xf703 + number))))
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: scalar, modifiers: .shift), .key("shift+f\(number)"))
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: scalar), .key("f\(number)"), "unmodified key unchanged")
        }
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "\u{f728}", modifiers: .shift), .key("shift+delete"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event("\u{f70d}", code: 0x6d, flags: [.shift, .function])),
                       .key("shift+f10"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event("\u{f728}", code: 0x75, flags: [.shift, .function])),
                       .key("shift+delete"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "\u{7f}", modifiers: .shift), .ignored, "Shift+Backspace unchanged")
    }

    func testCommandStaysHostReservedAndUnsupportedMixesStayIgnored() throws {
        let cases: [(String, UInt16, NSEvent.ModifierFlags)] = [
            ("s", 0x01, .command), ("\u{13}", 0x01, [.command, .control]), ("\r", 0x24, [.command, .control]),
            ("\t", 0x30, [.command, .control]), ("\u{f70d}", 0x6d, [.command, .shift]), ("\u{f728}", 0x75, [.command, .shift]),
            ("\u{3}", 0x4c, [.command, .control]), ("\u{13}", 0x01, [.control, .shift]), ("\u{13}", 0x01, [.control, .option]),
            ("\r", 0x24, [.control, .option]), ("\t", 0x30, [.control, .option]), ("\r", 0x24, [.control, .shift]),
            ("\u{3}", 0x4c, [.control, .shift]), ("\u{f70d}", 0x6d, .control), ("\u{f70d}", 0x6d, .option),
            ("\u{f70d}", 0x6d, [.shift, .option]), ("1", 0x12, .control), ("\u{1b}", 0x21, .control)
        ]
        for (characters, code, flags) in cases {
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event(characters, code: code, flags: flags)), .ignored,
                           "\(characters.unicodeScalars.map(\.value)) flags=\(flags.rawValue)")
        }
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "\u{7f}", modifiers: [.control, .option]), .key("ctrl+alt+delete"))
    }

    private func event(_ characters: String, code: UInt16, flags: NSEvent.ModifierFlags = .control) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0,
            windowNumber: 0, context: nil, characters: characters, charactersIgnoringModifiers: characters,
            isARepeat: false, keyCode: code))
    }
}
