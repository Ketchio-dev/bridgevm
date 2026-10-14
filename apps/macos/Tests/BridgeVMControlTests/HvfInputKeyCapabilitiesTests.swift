import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfInputKeyCapabilitiesTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 5_000)
    private let id = UUID(uuidString: "12345678-1234-1234-1234-123456789abc")!
    private var header: String { "BVAGENT CMD INPUTCAPS \(id.uuidString) exit=0" }
    private var base: String { "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536" }
    private var keys: String { "BVINPUT_KEYS \(id.uuidString) SHIFT_ENTER_SPACE" }
    private var end: String { "BVAGENT END INPUTCAPS \(id.uuidString)" }

    func testExtensionRequiresCompleteSuccessfulEnvelope() {
        var request = HvfInputCapabilitiesRequest(now: now, id: id)
        for line in [header, base, keys] {
            XCTAssertNil(request.consume(lines: [line + "\r"], now: now))
            XCTAssertFalse(request.supports(.key("shift+enter")))
        }
        XCTAssertEqual(request.consume(lines: [end], now: now), .supported)
        XCTAssertTrue(request.supports(.key("shift+enter")))
        XCTAssertTrue(request.supports(.key("shift+space")))
    }

    func testBaseV3RetainsOrdinaryInputButDoesNotImplyNewChords() {
        var request = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(request.consume(lines: [header, base, end], now: now), .supported)
        XCTAssertTrue(request.supports(.text("한글")))
        XCTAssertTrue(request.supports(.key("shift+tab")))
        XCTAssertFalse(request.supports(.key("shift+enter")))
        XCTAssertFalse(request.supports(.key("shift+space")))
    }

    func testForeignMalformedDuplicatedAndOutOfOrderExtensionsFailClosed() {
        for lines in [
            [keys, header, base, end], [header, keys, base, end],
            [header, base, keys, keys, end],
            [header, base, keys.replacingOccurrences(of: id.uuidString, with: UUID().uuidString), end],
            [header, base, keys + " UNKNOWN", end],
            [header, base, keys.replacingOccurrences(of: "SHIFT_ENTER_SPACE", with: "SHIFT_ENTER"), end],
            [header, base, keys, "BVAGENT CMD INPUTCAPS \(id.uuidString) exit=1", end]
        ] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: lines, now: now), .failed(.invalidReceipt))
            XCTAssertFalse(request.supports(.key("shift+enter")))
        }
    }

    func testExtensionAfterEnvelopeCannotUpgradeCompletedBaseReceipt() {
        for sameBatch in [false, true] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: [header, base, end] + (sameBatch ? [keys] : []), now: now), .supported)
            XCTAssertNil(request.consume(lines: [keys], now: now))
            XCTAssertFalse(request.supports(.key("shift+enter")))
        }
    }

    func testResetOrExpiryCannotFinishExpandedReceipt() {
        for reset in ["BVAGENT SERVICE start", "PSCI SYSTEM_RESET: reboot 1/8"] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: [header, base, keys, end, reset], now: now), .failed(.restarted))
            XCTAssertFalse(request.supports(.key("shift+space")))
        }
        var request = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertNil(request.consume(lines: [header, base, keys], now: now))
        XCTAssertEqual(request.consume(lines: [end], now: now.addingTimeInterval(30)), .failed(.expired))
        XCTAssertFalse(request.supports(.key("shift+space")))
    }

    func testServiceResetRenegotiatesInsteadOfRetainingExpandedSupport() {
        var stream = HvfNegotiatedInputStream()
        negotiate(&stream, expanded: true)
        _ = stream.reset(.targetChanged)
        XCTAssertEqual(stream.enqueue(.key("shift+space"), now: now), .queued)
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: ["BVAGENT SERVICE start"], now: now) { _ in false }
        XCTAssertEqual(stream.state, .disconnected)
        negotiate(&stream, expanded: false)
        XCTAssertEqual(stream.enqueue(.key("shift+space"), now: now), .refused)
        XCTAssertEqual(stream.enqueue(.text("still usable"), now: now), .queued)
    }

    private func negotiate(_ stream: inout HvfNegotiatedInputStream, expanded: Bool) {
        var command = ""
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ").last ?? "missing"
        var lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]
        if expanded { lines.append("BVINPUT_KEYS \(id) SHIFT_ENTER_SPACE") }
        lines.append("BVAGENT END \(command)")
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: lines, now: now) { _ in false }
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(stream.state, .ready)
    }
}
