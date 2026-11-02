// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HvfHostKeyCommand.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfHostKeyEventCommand.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfNonLatinControlShortcut.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfControlChordKeys.swift
import AppKit
import SwiftUI
import Testing

@MainActor
@Suite("Non-Latin Ctrl shortcuts")
struct HvfNonLatinControlShortcutTests {
    @Test func supportedPositionsAcceptSingleNonLatinLetters() throws {
        let cases: [(String, UInt16, String)] = [
            ("ㅁ", 0x00, "a"), ("ㅊ", 0x08, "c"), ("ㅍ", 0x09, "v"),
            ("ㅌ", 0x07, "x"), ("ㅛ", 0x10, "y"), ("ㅋ", 0x06, "z"),
            ("с", 0x08, "c"), ("α", 0x00, "a")
        ]
        for (text, code, key) in cases {
            #expect(HvfHostKeyCommand.resolve(event: try event(text, code: code)) == .key("ctrl+" + key))
        }
    }

    @Test func logicalCommandsAndUnsupportedEventsKeepTheirMeaning() throws {
        let cases: [(String, UInt16, NSEvent.ModifierFlags, HvfHostKeyCommand)] = [
            ("c", 0x00, .control, .key("ctrl+c")),
            ("\u{3}", 0x00, .control, .key("ctrl+c")),
            ("1", 0x08, .control, .ignored), ("💬", 0x08, .control, .ignored), ("\u{301}", 0x08, .control, .ignored),
            ("한글", 0x08, .control, .ignored), ("ㅎᅡ", 0x08, .control, .ignored),
            ("", 0x08, .control, .ignored), ("ㅊ", 0x12, .control, .ignored),
            ("ㅊ", 0x08, .command, .ignored),
            ("ㅊ", 0x08, [.command, .control], .ignored),
            ("ㅊ", 0x08, [.control, .shift], .ignored),
            ("ㅊ", 0x08, [.control, .option], .ignored),
            ("\u{3}", 0x4c, [.control, .shift], .ignored),
            ("ㅊ", 0x08, [], .text("ㅊ")), ("A", 0x00, .shift, .text("A"))
        ]
        for (text, code, flags, expected) in cases {
            #expect(HvfHostKeyCommand.resolve(event: try event(text, code: code, flags: flags)) == expected)
        }
    }

    private func event(_ text: String, code: UInt16, flags: NSEvent.ModifierFlags = .control) throws -> NSEvent {
        try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags,
            timestamp: 0, windowNumber: 0, context: nil, characters: text,
            charactersIgnoringModifiers: text, isARepeat: false, keyCode: code))
    }
}
