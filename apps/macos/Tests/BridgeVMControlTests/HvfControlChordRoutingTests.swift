import Foundation
import XCTest
@testable import BridgeVMControl

/// An old resident refuses unknown keys and drops queued input, so the host must never send one.
final class HvfControlChordRoutingTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 7_000)
    private let binding = ["disk", "vars", "evidence", "control"]
    private let chords = ["ctrl+s", "ctrl+f", "ctrl+n", "ctrl+o", "ctrl+p", "ctrl+w", "ctrl+t", "ctrl+l", "ctrl+h",
                          "ctrl+r", "ctrl+enter", "ctrl+tab", "ctrl+shift+tab", "shift+f10", "shift+delete"]

    func testOldAgentRefusesNewChordsBeforeQueueingAndKeepsOrderedInput() throws {
        for shiftMarker in [false, true] {
            var router = HvfSessionInputRouter()
            router.beginOwnedBoot(binding: binding)
            negotiate(&router, shift: shiftMarker, chords: false)
            XCTAssertEqual(router.route(.text("before"), binding: binding, now: now), .queued)
            for key in chords {
                XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .refused, key)
                XCTAssertEqual(router.count, 1, key)
                XCTAssertEqual(router.state, .ready)
                XCTAssertFalse(router.failed)
            }
            XCTAssertEqual(router.route(.key("ctrl+c"), binding: binding, now: now), .queued, "existing chord unchanged")
            XCTAssertEqual(router.route(.text("after"), binding: binding, now: now), .queued)
            for (verb, value, count) in [("TEXTINPUT", "before", 12), ("KEYINPUT", "ctrl+c", 4), ("TEXTINPUT", "after", 10)] {
                try deliverNext(&router, verb: verb, value: value, count: count)
            }
            XCTAssertEqual(router.count, 0)
            XCTAssertFalse(router.failed)
        }
    }

    func testChordAgentQueuesEveryNewChordAndTargetChangeKeepsSupport() throws {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router, shift: true, chords: true)
        for key in HvfControlChordKeys.extended.sorted() {
            _ = router.cancelTarget(now: now)
            XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .queued, key)
            try deliverNext(&router, verb: "KEYINPUT", value: key, count: key == "ctrl+shift+tab" ? 6 : 4)
        }
        XCTAssertEqual(router.route(.key("shift+enter"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.state, .ready)
        XCTAssertFalse(router.failed)
    }

    func testFreshOwnedBootDoesNotInheritChordSupport() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router, shift: true, chords: true)
        router.beginOwnedBoot(binding: binding)
        negotiate(&router, shift: true, chords: false)
        XCTAssertEqual(router.route(.key("ctrl+s"), binding: binding, now: now), .refused)
        XCTAssertEqual(router.route(.key("shift+enter"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.route(.text("still available"), binding: binding, now: now), .queued)
    }

    func testNewChordsUseLegacyHidBeforeOrderedActivation() {
        var router = HvfSessionInputRouter()
        router.attachUnknown(binding: binding)
        for key in chords { XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .legacy, key) }
    }

    private func negotiate(_ router: inout HvfSessionInputRouter, shift: Bool, chords: Bool) {
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ").last.map(String.init) ?? "missing"
        var lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]
        if shift { lines.append("BVINPUT_KEYS \(id) SHIFT_ENTER_SPACE") }
        if chords { lines.append("BVINPUT_CHORDS \(id) CTRL_SHIFT_CHORDS") }
        lines.append("BVAGENT END \(command)")
        _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(router.state, .ready)
    }

    private func deliverNext(_ router: inout HvfSessionInputRouter, verb: String, value: String, count: Int) throws {
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let fields = command.split(separator: " ").map(String.init)
        XCTAssertEqual(fields.count, 3, command)
        XCTAssertEqual(fields.first, verb, value)
        XCTAssertEqual(fields.last, Data(value.utf8).base64EncodedString(), value)
        let id = try XCTUnwrap(fields.dropFirst().first)
        let label = "\(verb) \(id)"
        let receipt = ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(id) \(count)", "BVAGENT END \(label)"]
        _ = router.poll(binding: binding, serviceReady: true, lines: receipt, now: now) { _ in XCTFail("receipt replay"); return false }
        XCTAssertFalse(router.failed, value)
    }
}
