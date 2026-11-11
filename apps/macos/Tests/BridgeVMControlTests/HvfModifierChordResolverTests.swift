import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

final class HvfModifierChordResolverTests: XCTestCase {
    private let mac = HvfHostKeyMapping(commandKey: .mac, modifierChords: true)
    private let windows = HvfHostKeyMapping(commandKey: .windows, modifierChords: true)
    private let older = HvfHostKeyMapping(commandKey: .mac, modifierChords: false)
    private let rightCommand = NSEvent.ModifierFlags(rawValue: NSEvent.ModifierFlags.command.rawValue | 0x10)

    private func chord(_ keyCode: UInt16, _ characters: String, _ modifiers: EventModifiers,
                       _ mapping: HvfHostKeyMapping) -> HvfHostKeyCommand? {
        HvfHostKeyCommand.resolveChord(keyCode: keyCode, charactersIgnoringModifiers: characters,
                                       modifiers: modifiers, mapping: mapping)
    }

    /// keyCode, charactersIgnoringModifiers, modifiers, Mac-like action, Windows-key action.
    func testCommandMapsToCtrlOrTheWindowsKeyByMode() {
        let rows: [(UInt16, String, EventModifiers, String, String)] = [
            (0x08, "c", .command, "ctrl+c", "win+c"),
            (0x09, "v", .command, "ctrl+v", "win+v"),
            (0x0e, "e", .command, "ctrl+e", "win+e"),
            (0x0f, "r", .command, "ctrl+r", "win+r"),
            (0x02, "d", .command, "ctrl+d", "win+d"),
            (0x25, "l", .command, "ctrl+l", "win+l"),
            (0x12, "1", .command, "ctrl+1", "win+1"),
            (0x1d, "0", .command, "ctrl+0", "win+0"),
            (0x01, "S", [.command, .shift], "ctrl+shift+s", "win+shift+s"),
            (0x11, "T", [.command, .shift], "ctrl+shift+t", "win+shift+t"),
            (0x13, "@", [.command, .shift], "ctrl+shift+2", "win+shift+2"),
            (0x7b, "\u{f702}", .command, "home", "win+left"),
            (0x2f, ".", .command, "ctrl+period", "win+period"),
            (0x18, "=", .command, "ctrl+equal", "win+equal"),
            (0x02, "d", [.command, .control], "ctrl+d", "win+ctrl+d"),
            (0x0e, "e", [.command, .option], "ctrl+alt+e", "win+alt+e")
        ]
        for (keyCode, characters, modifiers, macAction, windowsAction) in rows {
            XCTAssertEqual(chord(keyCode, characters, modifiers, mac), .key(macAction), macAction)
            XCTAssertEqual(chord(keyCode, characters, modifiers, windows), .key(windowsAction), windowsAction)
        }
    }

    func testOptionIsAltAndControlChordsCoverEveryKey() {
        let rows: [(UInt16, String, EventModifiers, String)] = [
            (0x01, "s", .option, "alt+s"),
            (0x03, "F", [.option, .shift], "alt+shift+f"),
            (0x31, " ", .option, "alt+space"),
            (0x76, "\u{f707}", .option, "alt+f4"),
            (0x30, "\t", .option, "alt+tab"),
            (0x24, "\r", .option, "alt+enter"),
            (0x7e, "\u{f700}", .option, "alt+up"),
            (0x12, "1", .control, "ctrl+1"),
            (0x19, "9", .control, "ctrl+9"),
            (0x00, "A", [.control, .shift], "ctrl+shift+a"),
            (0x35, "\u{1b}", [.control, .shift], "ctrl+shift+esc"),
            (0x35, "\u{1b}", .control, "ctrl+esc"),
            (0x76, "\u{f707}", .control, "ctrl+f4"),
            (0x2c, "/", .control, "ctrl+slash"),
            (0x1b, "-", .control, "ctrl+minus"),
            (0x08, "c", .control, "ctrl+c"),
            (0x24, "\r", .control, "ctrl+enter"),
            (0x4c, "\u{3}", .control, "ctrl+enter"),
            (0x30, "\t", [.control, .shift], "ctrl+shift+tab"),
            (0x33, "\u{7f}", .control, "ctrl+backspace"),
            (0x7c, "\u{f703}", [.control, .shift], "ctrl+shift+right"),
            (0x33, "\u{7f}", [.control, .option], "ctrl+alt+delete"),
            (0x33, "\u{7f}", [.control, .option, .shift], "ctrl+alt+delete"),
            (0x75, "\u{f728}", [.control, .option], "ctrl+alt+delete"),
            (0x32, "`", [.control, .option, .shift], "ctrl+alt+shift+grave")
        ]
        for (keyCode, characters, modifiers, action) in rows {
            XCTAssertEqual(chord(keyCode, characters, modifiers, mac), .key(action), action)
            XCTAssertEqual(chord(keyCode, characters, modifiers, windows), .key(action), "\(action): Command not held")
        }
    }

    /// 'Mac처럼' text navigation does what it does on a Mac; 'Windows 키로' keeps the literal chord.
    func testMacLikeTextNavigationFollowsTheMacAndWindowsKeyModeStaysLiteral() {
        let rows: [(UInt16, String, EventModifiers, String, String)] = [
            (0x7b, "\u{f702}", .command, "home", "win+left"),
            (0x7c, "\u{f703}", .command, "end", "win+right"),
            (0x7e, "\u{f700}", .command, "ctrl+home", "win+up"),
            (0x7d, "\u{f701}", .command, "ctrl+end", "win+down"),
            (0x7b, "\u{f702}", [.command, .shift], "shift+home", "win+shift+left"),
            (0x7c, "\u{f703}", [.command, .shift], "shift+end", "win+shift+right"),
            (0x7e, "\u{f700}", [.command, .shift], "ctrl+shift+home", "win+shift+up"),
            (0x7d, "\u{f701}", [.command, .shift], "ctrl+shift+end", "win+shift+down"),
            (0x7b, "\u{f702}", .option, "ctrl+left", "alt+left"),
            (0x7c, "\u{f703}", .option, "ctrl+right", "alt+right"),
            (0x7b, "\u{f702}", [.option, .shift], "ctrl+shift+left", "alt+shift+left"),
            (0x7c, "\u{f703}", [.option, .shift], "ctrl+shift+right", "alt+shift+right"),
            (0x33, "\u{7f}", .option, "ctrl+backspace", "alt+backspace"),
            // Not in the table: the generic chord in both modes.
            (0x33, "\u{7f}", .command, "ctrl+backspace", "win+backspace"),
            (0x7b, "\u{f702}", [.command, .option], "ctrl+alt+left", "win+alt+left"),
            (0x7b, "\u{f702}", [.command, .control], "ctrl+left", "win+ctrl+left"),
            (0x7b, "\u{f702}", .control, "ctrl+left", "ctrl+left"),
            (0x33, "\u{7f}", [.control, .option], "ctrl+alt+delete", "ctrl+alt+delete")
        ]
        for (keyCode, characters, modifiers, macAction, windowsAction) in rows {
            XCTAssertEqual(chord(keyCode, characters, modifiers, mac), .key(macAction), "\(modifiers) \(macAction)")
            XCTAssertEqual(chord(keyCode, characters, modifiers, windows), .key(windowsAction), "\(modifiers) \(windowsAction)")
        }
        for action in HvfHostKeyCommand.macTextNavigation.values {
            XCTAssertTrue(HvfModifierChord.isBase(action), "\(action) also reaches agents without the marker")
        }
    }

    /// Korean 2-Set reports Hangul jamo for letters, so the ANSI key position names them;
    /// a Latin layout's letter (here Dvorak's C on the ANSI I key) stays the letter it types.
    func testNonLatinLettersFallBackToKeyPositionAndLatinLettersStayLogical() {
        let rows: [(UInt16, String, EventModifiers, String)] = [
            (0x08, "ㅊ", .command, "ctrl+c"),
            (0x09, "ㅍ", .command, "ctrl+v"),
            (0x06, "ㅋ", .command, "ctrl+z"),
            (0x01, "ㄴ", [.command, .shift], "ctrl+shift+s"),
            (0x0e, "ㄷ", .option, "alt+e"),
            (0x0f, "ㄱ", .control, "ctrl+r"),
            (0x00, "ㅁ", [.control, .shift], "ctrl+shift+a"),
            (0x12, "1", .command, "ctrl+1"),
            (0x22, "c", .command, "ctrl+c"),
            (0x08, "Ж", .control, "ctrl+c")
        ]
        for (keyCode, characters, modifiers, action) in rows {
            XCTAssertEqual(chord(keyCode, characters, modifiers, mac), .key(action), "\(characters) \(action)")
        }
        XCTAssertEqual(chord(0x0e, "ㄷ", .command, windows), .key("win+e"))
    }

    func testMacAppShortcutsStayWithTheMacInBothModes() {
        for mapping in [mac, windows] {
            for (keyCode, characters, modifiers) in [(UInt16(0x0c), "q", EventModifiers.command), (0x0c, "ㅂ", .command), (0x04, "h", .command),
                                                     (0x2e, "m", .command), (0x2b, ",", .command), (0x03, "f", [.command, .control]),
                                                     (0x30, "\t", .command), (0x30, "\t", [.command, .shift])] {
                XCTAssertEqual(chord(keyCode, characters, modifiers, mapping), .ignored, "\(characters) \(modifiers)")
            }
            // Only those exact chords: their other variants reach Windows.
            XCTAssertNotEqual(chord(0x0c, "Q", [.command, .shift], mapping), .ignored)
            XCTAssertNotEqual(chord(0x03, "f", .command, mapping), .ignored)
            XCTAssertNotEqual(chord(0x04, "h", [.command, .option], mapping), .ignored)
            XCTAssertNotEqual(chord(0x0c, "q", .control, mapping), .ignored)
        }
        XCTAssertEqual(chord(0x03, "f", .command, mac), .key("ctrl+f"))
        XCTAssertEqual(chord(0x2b, ",", .control, mac), .key("ctrl+comma"))
    }

    func testUnknownKeysAreIgnoredAndShiftOrUnmodifiedKeysKeepTheEarlierResolver() {
        XCTAssertEqual(chord(0x53, "1", .control, mac), .ignored, "keypad digits have no chord name")
        XCTAssertEqual(chord(0x6b, "\u{f711}", .command, mac), .ignored, "F14")
        for modifiers in [EventModifiers(), .shift] {
            XCTAssertNil(chord(0x00, "a", modifiers, mac))
            XCTAssertNil(chord(0x7a, "\u{f704}", modifiers, mac))
        }
    }

    /// Before the guest takes generic chords, every key resolves exactly as it did before them.
    func testWithoutGuestSupportCommandStaysWithTheMacAndOptionTypesText() throws {
        XCTAssertNil(chord(0x08, "c", .command, older))
        XCTAssertNil(chord(0x01, "s", .option, older))
        for (modifiers, characters, ignoring, keyCode, expected) in [
            (NSEvent.ModifierFlags.command, "c", "c", UInt16(0x08), HvfHostKeyCommand.ignored),
            (.command, "\u{1b}", "\u{1b}", 0x35, .ignored),
            (.option, "ß", "s", 0x01, .ignored),
            (.option, "\u{f707}", "\u{f707}", 0x76, .key("alt+f4")),
            (.control, "\u{13}", "s", 0x01, .key("ctrl+s")),
            (.control, "1", "1", 0x12, .ignored),
            ([.control, .shift], "\u{1}", "A", 0x00, .ignored),
            ([.command, .numericPad, .function], "\u{f702}", "\u{f702}", 0x7b, .ignored),
            ([.option, .numericPad, .function], "\u{f702}", "\u{f702}", 0x7b, .ignored)
        ] {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers,
                timestamp: 0, windowNumber: 0, context: nil, characters: characters,
                charactersIgnoringModifiers: ignoring, isARepeat: false, keyCode: keyCode))
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: event, mapping: older), expected, "\(modifiers) \(ignoring)")
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: event), expected, "the default mapping is the older one")
        }
    }

    func testAppKitEventsResolveByKeyCodeWithGuestSupport() throws {
        for (modifiers, characters, ignoring, keyCode, mapping, expected) in [
            (NSEvent.ModifierFlags.command, "c", "c", UInt16(0x08), mac, "ctrl+c"),
            (.command, "ㅊ", "ㅊ", 0x08, mac, "ctrl+c"),
            (.command, "e", "e", 0x0e, windows, "win+e"),
            ([.command, .shift], "S", "S", 0x01, windows, "win+shift+s"),
            (.option, "ß", "s", 0x01, mac, "alt+s"),
            (.option, "", "e", 0x0e, mac, "alt+e"),
            ([.control, .shift], "\u{1b}", "\u{1b}", 0x35, mac, "ctrl+shift+esc"),
            (.control, "1", "1", 0x12, mac, "ctrl+1"),
            ([.control, .numericPad], "\u{3}", "\u{3}", 0x4c, mac, "ctrl+enter"),
            ([.command, .numericPad, .function], "\u{f702}", "\u{f702}", 0x7b, mac, "home"),
            ([.option, .shift, .numericPad, .function], "\u{f703}", "\u{f703}", 0x7c, mac, "ctrl+shift+right"),
            ([.option, .numericPad, .function], "\u{f702}", "\u{f702}", 0x7b, windows, "alt+left"),
            // Right Command carries its device bit (NX_DEVICERCMDKEYMASK); its chords map like the left key.
            (rightCommand, "c", "c", 0x08, mac, "ctrl+c"),
            (rightCommand, "e", "e", 0x0e, windows, "win+e"),
            (rightCommand.union(.shift), "S", "S", 0x01, mac, "ctrl+shift+s")
        ] {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers,
                timestamp: 0, windowNumber: 0, context: nil, characters: characters,
                charactersIgnoringModifiers: ignoring, isARepeat: false, keyCode: keyCode))
            XCTAssertEqual(HvfHostKeyCommand.resolve(event: event, mapping: mapping), .key(expected), expected)
        }
    }

    func testEveryResolvedChordIsAValidGuestAction() {
        let keyCodes: [UInt16] = Array(0x00...0x32) + [0x35, 0x4c, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x67, 0x6d, 0x6f,
                                                       0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e]
        let combinations: [EventModifiers] = [.command, [.command, .shift], [.command, .control, .option, .shift],
                                              .control, [.control, .shift], .option, [.option, .shift], [.control, .option]]
        for keyCode in keyCodes {
            for modifiers in combinations {
                for mapping in [mac, windows] {
                    guard case let .key(action)? = chord(keyCode, "", modifiers, mapping) else { continue }
                    XCTAssertNotNil(HvfModifierChord.eventCount(action), action)
                }
            }
        }
    }
}
