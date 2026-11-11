import Foundation
import XCTest
@testable import BridgeVMControl

/// An old resident refuses unknown keys and drops queued input, so the host sends a generic chord
/// only through the VMM or to an agent that advertised MODIFIER_CHORDS.
final class HvfModifierChordCapabilityTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 8_000)
    private let binding = ["disk", "vars", "evidence", "control"]
    private let generic = ["win", "alt", "win+e", "win+shift+s", "alt+s", "ctrl+1", "ctrl+shift+esc", "ctrl+shift+a",
                           "ctrl+esc", "win+l", "ctrl+slash", "win+ctrl+alt+shift+f12", "shift+backspace"]

    func testGenericChordsEncodeWithTheirEventCountsAndCtrlAltDeleteNever() throws {
        for (key, count) in [("win", 2), ("alt", 2), ("win+e", 4), ("win+shift+s", 6), ("ctrl+shift+esc", 6),
                             ("win+ctrl+alt+shift+f12", 10), ("ctrl+c", 4), ("shift+f10", 4), ("enter", 2), ("ctrl+period", 4)] {
            let encoding = try XCTUnwrap(HvfGuestInputEncoding(.key(key)), key)
            XCTAssertEqual(encoding.verb, "KEYINPUT")
            XCTAssertEqual(Data(base64Encoded: encoding.base64), Data(key.utf8))
            XCTAssertEqual(encoding.insertedEventCount, count, key)
        }
        for key in ["ctrl+alt+delete", "ctrl+alt+shift+delete", "win+ctrl+alt+delete", "a", "shift+a", "shift+1", "Win",
                    "shift+win+e", "ctrl+ctrl+v", "ctrl+win", "ctrl+", "ctrl+,", "ctrl+f13", "command+c", "ctrl+" + String(repeating: "a", count: 28)] {
            XCTAssertNil(HvfGuestInputEncoding(.key(key)), key)
        }
    }

    func testOlderAgentsKeepTheirKeysAndRefuseGenericChords() {
        for markers in [[], ["BVINPUT_KEYS"], ["BVINPUT_KEYS", "CTRL_SHIFT_CHORDS"]] {
            var router = negotiated(markers)
            XCTAssertFalse(router.modifierChords)
            XCTAssertEqual(router.route(.text("before"), binding: binding, now: now), .queued)
            for key in generic {
                XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .refused, key)
                XCTAssertEqual(router.count, 1, key)
                XCTAssertFalse(router.failed, key)
            }
            XCTAssertEqual(router.route(.key("ctrl+c"), binding: binding, now: now), .queued, "existing chord unchanged")
            XCTAssertEqual(router.route(.key("ctrl+s"), binding: binding, now: now),
                           markers.count == 2 ? .queued : .refused)
        }
    }

    func testModifierChordMarkerAdmitsEveryChordAndImpliesTheEarlierKeysets() throws {
        var router = negotiated(["MODIFIER_CHORDS"])
        XCTAssertTrue(router.modifierChords)
        for key in generic + ["shift+enter", "ctrl+s", "shift+f10", "ctrl+c", "alt+f4"] {
            _ = router.cancelTarget(now: now)
            XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .queued, key)
            try deliverNext(&router, value: key, count: try XCTUnwrap(HvfModifierChord.eventCount(key)))
        }
        XCTAssertEqual(router.route(.key("ctrl+alt+delete"), binding: binding, now: now), .legacy, "the VMM keyboard raises it")
        XCTAssertFalse(router.failed)
        XCTAssertEqual(router.state, .ready)
    }

    func testTheLegacyRouteAlwaysTakesGenericChords() {
        var attached = HvfSessionInputRouter()
        attached.attachUnknown(binding: binding)
        XCTAssertTrue(attached.modifierChords)
        for key in generic { XCTAssertEqual(attached.route(.key(key), binding: binding, now: now), .legacy, key) }
        var booting = HvfSessionInputRouter()
        booting.beginOwnedBoot(binding: binding)
        XCTAssertTrue(booting.modifierChords, "until the agent negotiates, input goes through the VMM")
        _ = booting.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in true }
        XCTAssertTrue(booting.modifierChords, "still negotiating")
    }

    func testMisplacedOrMisspelledModifierMarkerGrantsNothing() {
        let id = UUID()
        for marker in ["BVINPUT_CHORDS \(id.uuidString) MODIFIER_CHORDS EXTRA", "BVINPUT_CHORDS \(id.uuidString) MODIFIER_CHORD",
                       "BVINPUT_KEYS \(id.uuidString) MODIFIER_CHORDS", "BVINPUT_CHORDS \(UUID().uuidString) MODIFIER_CHORDS"] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            let lines = ["BVAGENT CMD INPUTCAPS \(id.uuidString) exit=0",
                         "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", marker,
                         "BVAGENT END INPUTCAPS \(id.uuidString)"]
            XCTAssertEqual(request.consume(lines: lines, now: now), .supported, marker)
            XCTAssertFalse(request.modifierChords, marker)
            XCTAssertFalse(request.supports(.key("win")), marker)
            XCTAssertTrue(request.supports(.key("ctrl+c")), marker)
        }
        var early = HvfInputCapabilitiesRequest(now: now, id: id)
        let marker = "BVINPUT_CHORDS \(id.uuidString) MODIFIER_CHORDS"
        XCTAssertEqual(early.consume(lines: [marker, "BVAGENT CMD INPUTCAPS \(id.uuidString) exit=0",
                                             "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                                             "BVAGENT END INPUTCAPS \(id.uuidString)"], now: now), .supported)
        XCTAssertFalse(early.supports(.key("win")), "a marker before BVINPUT_CAPS is ignored")
    }

    private func negotiated(_ markers: [String]) -> HvfSessionInputRouter {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ").last.map(String.init) ?? "missing"
        var lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]
        for marker in markers {
            switch marker {
            case "BVINPUT_KEYS": lines.append("BVINPUT_KEYS \(id) SHIFT_ENTER_SPACE")
            default: lines.append("BVINPUT_CHORDS \(id) \(marker)")
            }
        }
        lines.append("BVAGENT END \(command)")
        _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(router.state, .ready)
        return router
    }

    private func deliverNext(_ router: inout HvfSessionInputRouter, value: String, count: Int) throws {
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let fields = command.split(separator: " ").map(String.init)
        XCTAssertEqual(fields.count, 3, command)
        XCTAssertEqual(fields.first, "KEYINPUT", value)
        XCTAssertEqual(fields.last, Data(value.utf8).base64EncodedString(), value)
        let id = try XCTUnwrap(fields.dropFirst().first)
        let label = "KEYINPUT \(id)"
        let receipt = ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(id) \(count)", "BVAGENT END \(label)"]
        _ = router.poll(binding: binding, serviceReady: true, lines: receipt, now: now) { _ in XCTFail("receipt replay"); return false }
        XCTAssertFalse(router.failed, value)
    }
}
