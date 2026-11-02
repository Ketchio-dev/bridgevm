import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfControlChordCapabilityTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 6_000)
    private let id = UUID(uuidString: "12345678-1234-1234-1234-123456789abc")!
    private var header: String { "BVAGENT CMD INPUTCAPS \(id.uuidString) exit=0" }
    private var base: String { "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536" }
    private var keys: String { "BVINPUT_KEYS \(id.uuidString) SHIFT_ENTER_SPACE" }
    private var chords: String { "BVINPUT_CHORDS \(id.uuidString) CTRL_SHIFT_CHORDS" }
    private var end: String { "BVAGENT END INPUTCAPS \(id.uuidString)" }
    private let existing = ["ctrl+a", "ctrl+c", "ctrl+v", "ctrl+x", "ctrl+y", "ctrl+z", "ctrl+left", "ctrl+shift+home",
                            "ctrl+backspace", "ctrl+delete", "shift+tab", "shift+left", "f10", "delete", "enter", "alt+f4"]

    func testExtendedSetIsExactlyTheNewChordsAndEachEncodes() throws {
        let letters = "bdefghijklmnopqrstuw".map { "ctrl+\($0)" }
        let expected = Set(letters + (1...12).map { "shift+f\($0)" } + ["ctrl+enter", "ctrl+tab", "ctrl+shift+tab", "shift+delete"])
        XCTAssertEqual(HvfControlChordKeys.extended, expected)
        XCTAssertEqual(expected.count, 36)
        XCTAssertTrue(HvfControlChordKeys.extended.isDisjoint(with: existing))
        for key in expected {
            let encoding = try XCTUnwrap(HvfGuestInputEncoding(.key(key)), key)
            XCTAssertEqual(encoding.verb, "KEYINPUT")
            XCTAssertEqual(Data(base64Encoded: encoding.base64), Data(key.utf8))
            XCTAssertEqual(encoding.insertedEventCount, key == "ctrl+shift+tab" ? 6 : 4, key)
        }
    }

    func testOlderAgentsKeepExistingChordsButNeverAdmitNewOnes() {
        for extra in [[], [keys]] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: [header, base] + extra + [end], now: now), .supported)
            for key in existing { XCTAssertTrue(request.supports(.key(key)), key) }
            for key in HvfControlChordKeys.extended { XCTAssertFalse(request.supports(.key(key)), key) }
            XCTAssertEqual(request.supports(.key("shift+enter")), !extra.isEmpty)
            XCTAssertTrue(request.supports(.text("한글")))
        }
    }

    func testChordMarkerAdmitsNewChordsWithOrWithoutShiftMarker() {
        for lines in [[header, base, keys, chords, end], [header, base, chords + "\r", end]] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            for line in lines.dropLast() {
                XCTAssertNil(request.consume(lines: [line], now: now))
                XCTAssertFalse(request.supports(.key("ctrl+s")), "only a completed envelope grants support")
            }
            XCTAssertEqual(request.consume(lines: [end], now: now), .supported)
            for key in HvfControlChordKeys.extended.union(existing) { XCTAssertTrue(request.supports(.key(key)), key) }
            XCTAssertEqual(request.supports(.key("shift+space")), lines.contains(keys))
        }
    }

    func testMisplacedOrMisspelledChordMarkerIsIgnoredAndRepeatsOrOrderDoNotMatter() {
        for lines in [
            [chords, header, base, end], [header, chords, base, end],
            [header, base, chords + " EXTRA", end], [header, base, keys + " " + "CTRL_SHIFT_CHORDS", end],
            [header, base, chords.replacingOccurrences(of: "CTRL_SHIFT_CHORDS", with: "CTRL_CHORDS"), end],
            [header, base, chords.replacingOccurrences(of: id.uuidString, with: UUID().uuidString), end]
        ] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: lines, now: now), .supported, "\(lines)")
            XCTAssertFalse(request.supports(.key("ctrl+s")), "\(lines)")
            XCTAssertTrue(request.supports(.key("ctrl+c")), "base chords stay available")
        }
        for lines in [[header, base, chords, keys, end], [header, base, chords, chords, end],
                      [header, base, keys, chords, keys, end]] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: lines, now: now), .supported, "\(lines)")
            XCTAssertTrue(request.supports(.key("ctrl+s")), "\(lines)")
        }
        var reheaded = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(reheaded.consume(lines: [header, base, chords, "BVAGENT CMD INPUTCAPS \(id.uuidString) exit=1", end],
                                        now: now), .failed(.invalidReceipt))
        XCTAssertFalse(reheaded.supports(.key("ctrl+s")))
    }

    func testUnknownMarkerKeywordsAreIgnoredWithoutGrantingChords() {
        var request = HvfInputCapabilitiesRequest(now: now, id: id)
        let lines = [header, base, "BVINPUT_FUTURE \(id.uuidString) SOMETHING", chords.lowercased(), end]
        XCTAssertEqual(request.consume(lines: lines, now: now), .supported)
        XCTAssertTrue(request.supports(.key("ctrl+c")))
        XCTAssertFalse(request.supports(.key("ctrl+s")))
    }

    func testChordMarkerAfterEnvelopeOrAcrossRestartCannotUpgrade() {
        for sameBatch in [false, true] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: [header, base, end] + (sameBatch ? [chords] : []), now: now), .supported)
            XCTAssertNil(request.consume(lines: [chords], now: now))
            XCTAssertFalse(request.supports(.key("shift+f10")))
        }
        var restarted = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(restarted.consume(lines: [header, base, chords, end, "BVAGENT SERVICE start"], now: now), .failed(.restarted))
        XCTAssertFalse(restarted.supports(.key("ctrl+enter")))
        var expired = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertNil(expired.consume(lines: [header, base, chords], now: now))
        XCTAssertEqual(expired.consume(lines: [end], now: now.addingTimeInterval(30)), .failed(.expired))
        XCTAssertFalse(expired.supports(.key("ctrl+tab")))
    }
}
