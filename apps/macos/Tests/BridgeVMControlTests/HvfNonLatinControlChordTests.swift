import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// Korean 2-Set (Dubeolsik) on ANSI hardware: Ctrl events carry jamo, not Latin letters.
@MainActor
final class HvfNonLatinControlChordTests: XCTestCase {
    private let dubeolsik: [(String, UInt16, String)] = [
        ("ㅁ", 0x00, "a"), ("ㅠ", 0x0b, "b"), ("ㅊ", 0x08, "c"), ("ㅇ", 0x02, "d"), ("ㄷ", 0x0e, "e"),
        ("ㄹ", 0x03, "f"), ("ㅎ", 0x05, "g"), ("ㅗ", 0x04, "h"), ("ㅑ", 0x22, "i"), ("ㅓ", 0x26, "j"),
        ("ㅏ", 0x28, "k"), ("ㅣ", 0x25, "l"), ("ㅡ", 0x2e, "m"), ("ㅜ", 0x2d, "n"), ("ㅐ", 0x1f, "o"),
        ("ㅔ", 0x23, "p"), ("ㅂ", 0x0c, "q"), ("ㄱ", 0x0f, "r"), ("ㄴ", 0x01, "s"), ("ㅅ", 0x11, "t"),
        ("ㅕ", 0x20, "u"), ("ㅍ", 0x09, "v"), ("ㅈ", 0x0d, "w"), ("ㅌ", 0x07, "x"), ("ㅛ", 0x10, "y"),
        ("ㅋ", 0x06, "z")
    ]

    func testKoreanJamoFallsBackToEveryAnsiLetterPosition() throws {
        XCTAssertEqual(dubeolsik.map { $0.2 }.joined(), "abcdefghijklmnopqrstuvwxyz")
        for (jamo, code, letter) in dubeolsik {
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event(jamo, code: code)), .key("ctrl+" + letter), jamo)
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event(jamo, code: code, flags: [])), .text(jamo))
        }
    }

    func testOtherScriptsAndPhysicalKeysKeepPrecedenceUnderNonLatinSources() throws {
        let cases: [(String, UInt16, NSEvent.ModifierFlags, String)] = [
            ("ы", 0x01, .control, "ctrl+s"), ("σ", 0x01, .control, "ctrl+s"), ("ц", 0x0d, .control, "ctrl+w"),
            ("s", 0x00, .control, "ctrl+s"), ("\u{13}", 0x0d, .control, "ctrl+s"),
            ("\r", 0x24, .control, "ctrl+enter"), ("\t", 0x30, .control, "ctrl+tab"),
            ("\u{19}", 0x30, [.control, .shift], "ctrl+shift+tab")
        ]
        for (characters, code, flags, key) in cases {
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event(characters, code: code, flags: flags)), .key(key), key)
        }
    }

    func testNonLatinFallbackStaysCtrlOnlyAtLetterPositions() throws {
        let cases: [(String, UInt16, NSEvent.ModifierFlags)] = [
            ("ㄴ", 0x01, [.control, .shift]), ("ㄴ", 0x01, [.control, .option]), ("ㄴ", 0x01, [.command, .control]),
            ("ㄴ", 0x01, .command), ("ㄴ", 0x12, .control), ("ㄴ", 0x24, [.control, .option]),
            ("ㄴ", 0x30, [.control, .option]), ("ㄴㅁ", 0x01, .control), ("\u{301}", 0x01, .control), ("💬", 0x01, .control)
        ]
        for (characters, code, flags) in cases {
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: try event(characters, code: code, flags: flags)), .ignored,
                           "\(characters) \(code) \(flags.rawValue)")
        }
    }

    private func event(_ characters: String, code: UInt16, flags: NSEvent.ModifierFlags = .control) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0,
            windowNumber: 0, context: nil, characters: characters, charactersIgnoringModifiers: characters,
            isARepeat: false, keyCode: code))
    }
}
