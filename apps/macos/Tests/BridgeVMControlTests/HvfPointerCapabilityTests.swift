import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPointerCapabilityTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)

    private func receipt(_ command: String, capabilities: String) -> [String] {
        let id = command.split(separator: " ")[1]
        return ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) \(capabilities)",
                "BVAGENT END \(command)"]
    }

    func testAllThreeEncodingsAreRequired() {
        for capabilities in ["1 TEXTINPUT KEYINPUT POINTERINPUT 65536", "1 TEXTINPUT KEYINPUT 65536", "1 TEXTINPUT POINTERINPUT 65536",
                             "1 KEYINPUT POINTERINPUT 65536", "1 TEXTINPUT KEYINPUT POINTERINPUT 65535"] {
            var request = HvfInputCapabilitiesRequest(now: now)
            let lines = receipt(request.command, capabilities: capabilities)
            XCTAssertEqual(request.consume(lines: lines, now: now), .failed(.invalidReceipt))
        }
        var complete = HvfInputCapabilitiesRequest(now: now)
        let lines = receipt(complete.command, capabilities: "2 TEXTINPUT KEYINPUT POINTERINPUT 65536")
        XCTAssertEqual(complete.consume(lines: lines, now: now), .supported)
    }

    func testOldAgentCannotEnablePointerStreamEvenWhenLegacyIsQuiescent() {
        var stream = HvfNegotiatedInputStream()
        var command = ""
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) {
            command = $0; return true
        }
        let lines = receipt(command, capabilities: "1 TEXTINPUT KEYINPUT 65536")
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: lines, now: now) {
            _ in XCTFail("input sent during unsupported negotiation"); return true
        }
        XCTAssertEqual(stream.state, .unavailable)
        XCTAssertEqual(stream.enqueue(.pointer("click:0x0"), now: now), .legacy)
        XCTAssertEqual(stream.count, 0)
    }
}
