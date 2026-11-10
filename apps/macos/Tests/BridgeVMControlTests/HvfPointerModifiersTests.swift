#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

final class HvfPointerModifiersTests: XCTestCase {
    private let mac = HvfHostKeyMapping(commandKey: .mac), windows = HvfHostKeyMapping(commandKey: .windows)
    /// NX_DEVICELCMDKEYMASK / NX_DEVICERCMDKEYMASK with the Command flag, as hardware events carry them.
    private let leftCommand = NSEvent.ModifierFlags(rawValue: NSEvent.ModifierFlags.command.rawValue | 0x08)
    private let rightCommand = NSEvent.ModifierFlags(rawValue: NSEvent.ModifierFlags.command.rawValue | 0x10)

    func testFlagsMapAsKeysDoInEachCommandKeyStyle() {
        for mapping in [mac, windows] {
            XCTAssertEqual(mapping.pointerModifiers([]), "")
            XCTAssertEqual(mapping.pointerModifiers(.shift), "s")
            XCTAssertEqual(mapping.pointerModifiers(.control), "c")
            XCTAssertEqual(mapping.pointerModifiers(.option), "a")
            XCTAssertEqual(mapping.pointerModifiers([.control, .option, .shift]), "cas")
            XCTAssertEqual(mapping.pointerModifiers([.capsLock, .function, .numericPad, .help]), "", "not modifiers here")
            XCTAssertEqual(mapping.pointerModifiers([.capsLock, .shift]), "s")
        }
        // 'Mac처럼': Command is Ctrl, so ⌘-click is Ctrl+click, from either Command key.
        XCTAssertEqual(mac.pointerModifiers(leftCommand), "c")
        XCTAssertEqual(mac.pointerModifiers(rightCommand), "c")
        XCTAssertEqual(mac.pointerModifiers([.command, .control, .shift]), "cs")
        XCTAssertEqual(mac.pointerModifiers([.command, .option]), "ca")
        // 'Windows 키로': Command is left out (a Windows key up after a click opens Start), from either side.
        XCTAssertEqual(windows.pointerModifiers(leftCommand), "")
        XCTAssertEqual(windows.pointerModifiers(rightCommand), "")
        XCTAssertEqual(windows.pointerModifiers(rightCommand.union(.shift)), "s")
        XCTAssertEqual(windows.pointerModifiers([.command, .control, .option, .shift]), "cas")
        XCTAssertEqual(HvfHostKeyMapping(commandKey: .windows, modifierChords: true).pointerModifiers(leftCommand), "")
    }

    func testTheHeldModifierFieldIsStrictAndEveryKeyIsCounted() {
        for (command, count) in [("press:0x0:s", 3), ("click:32767x0:wcas", 10), ("releaseall:1x2:cs", 5),
                                 ("rightclick:1x2:a", 4), ("middlepress:0x0", 1), ("middlerelease:0x0", 1),
                                 ("middleclick:0x0", 2), ("middleclick:0x0:c", 4), ("scrollunits:-1200,1200@32767x32767:wcas", 11),
                                 ("scrollunits:120,0@0x0:c", 4), ("scroll:-1@0x0:s", 4), ("move:1x1:w", 3), ("wheel:1:a", 3)] {
            XCTAssertEqual(HvfPointerInputEncoding.eventCount(command), count, command)
            XCTAssertEqual(HvfGuestInputEncoding(.pointer(command))?.insertedEventCount, count, command)
        }
        for command in ["press:0x0:", "press:0x0:S", "press:0x0:sc", "press:0x0:ss", "press:0x0:aw", "press:0x0:x",
                        "press:0x0:wcasx", "press:0x0: s", "press:0x0:s ", "press:0x0:s\n", "press:0x0:s:c", "press:0x0::s",
                        "press::s", ":0x0:s", "press:0x0x0:s", "press:01x0:s", "doubleclick:0x0:s", "press:0x0:1",
                        "press:0x0:shift", "press:0x0:\u{0455}", "press:0x0:s\u{301}", "middleclick:0x0x0", "middleclick:32768x0",
                        "Middleclick:0x0", "middle:0x0", "middleclick:0x0:", "scrollunits:0,0@0x0:s", "scroll:0@0x0:c",
                        "click:0x0:" + String(repeating: "w", count: 60)] {
            XCTAssertNil(HvfPointerInputEncoding.eventCount(command), command)
        }
    }

    func testTheReceiptGrantsModifiersAndTheMiddleButtonOnlyWithTheirMarkers() {
        for markers in [[], ["MODIFIERS"], ["MIDDLE"], ["MODIFIERS", "MIDDLE"], ["SCROLL_UNITS"]] {
            let id = UUID()
            var receipt = HvfInputCapabilityReceipt(id: id)
            let lines = ["BVAGENT CMD \(receipt.command) exit=0", "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]
                + markers.map { "BVINPUT_POINTER \(id.uuidString) \($0)" } + ["BVINPUT_POINTER \(id.uuidString) MIDDLE_BUTTON",
                "BVINPUT_POINTER 00000000-0000-0000-0000-000000000000 MODIFIERS", "BVAGENT END \(receipt.command)"]
            XCTAssertEqual(lines.compactMap { receipt.consume($0) }, [.supported])
            let modifiers = markers.contains("MODIFIERS"), middle = markers.contains("MIDDLE")
            XCTAssertEqual(receipt.pointerModifiers, modifiers, "\(markers)")
            XCTAssertEqual(receipt.middleButton, middle, "\(markers)")
            XCTAssertEqual(receipt.supports(.pointer("press:1x2:cs")), modifiers, "\(markers)")
            XCTAssertEqual(receipt.supports(.pointer("scroll:1@1x2:c")), modifiers, "\(markers)")
            XCTAssertEqual(receipt.supports(.pointer("middlepress:1x2")), middle, "\(markers)")
            XCTAssertEqual(receipt.supports(.pointer("middleclick:1x2:c")), middle && modifiers, "\(markers)")
            XCTAssertTrue(receipt.supports(.pointer("press:1x2")), "\(markers)")
            XCTAssertTrue(receipt.supports(.pointer("releaseall:1x2")), "\(markers)")
        }
    }

    func testAModifiedOrMiddlePressIsReleasedAtItsPositionWithoutItsModifiers() {
        var held = HvfPointerHeldState()
        held.sent(.pointer("press:10x20:cs"))
        XCTAssertEqual(held.release, "releaseall:10x20")
        held.inserted(.pointer("press:10x20:cs"))
        held.sent(.pointer("releaseall:10x20:cs"))
        held.inserted(.pointer("releaseall:10x20:cs"))
        XCTAssertNil(held.release)
        held.sent(.pointer("middlepress:5x6:c"))
        XCTAssertEqual(held.release, "releaseall:5x6")
        held.sent(.pointer("scrollunits:1,0@7x8:c"))
        XCTAssertEqual(held.release, "releaseall:7x8")
        held.inserted(.pointer("middlerelease:7x8"))
        XCTAssertNil(held.release)
        held.sent(.pointer("middleclick:1x1:a"))
        held.inserted(.pointer("middleclick:1x1:a"))
        XCTAssertNil(held.release)
    }
}
#endif
