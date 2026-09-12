import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfInputCapabilitiesRequestTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 3_000)
    private let id = UUID(uuidString: "12345678-1234-1234-1234-123456789abc")!
    private var marker: String { "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536" }

    func testRequiresCompleteExactCapabilityReceipt() {
        var request = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(request.command, "INPUTCAPS \(id.uuidString)")
        XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0\r"], now: now))
        XCTAssertNil(request.consume(lines: [marker], now: now))
        XCTAssertEqual(request.consume(lines: ["BVAGENT END \(request.command)"], now: now), .supported)
        XCTAssertNil(request.consume(lines: [marker], now: now))
    }

    func testUnknownVersionMissingKeySupportAndWrongLimitFailClosed() {
        for value in ["2 TEXTINPUT KEYINPUT 65536", "1 TEXTINPUT 65536", "1 TEXTINPUT KEYINPUT 999999"] {
            var request = HvfInputCapabilitiesRequest(now: now, id: id)
            XCTAssertEqual(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0",
                "BVINPUT_CAPS \(id.uuidString) \(value)", "BVAGENT END \(request.command)"], now: now),
                .failed(.invalidReceipt))
        }
    }

    func testOldAgentFailureMissingMarkerAndTimeoutNeverEnableInput() {
        var rejected = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(rejected.consume(lines: ["BVAGENT CMD \(rejected.command) exit=1"], now: now),
                       .failed(.unsupported))
        var incomplete = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(incomplete.consume(lines: ["BVAGENT CMD \(incomplete.command) exit=0",
            "BVAGENT END \(incomplete.command)"], now: now), .failed(.invalidReceipt))
        var silent = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(silent.consume(lines: [], now: now.addingTimeInterval(30)), .failed(.expired))
    }

    func testRestartInvalidatesEarlierSuccessInSameBatch() {
        var request = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertEqual(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0", marker,
            "BVAGENT END \(request.command)", "BVAGENT re-READY"], now: now), .failed(.restarted))
    }

    func testForeignAndOutOfOrderReceiptsCannotEnableSupport() {
        var request = HvfInputCapabilitiesRequest(now: now, id: id)
        XCTAssertNil(request.consume(lines: ["BVINPUT_CAPS other 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"], now: now))
        XCTAssertEqual(request.consume(lines: [marker], now: now), .failed(.invalidReceipt))
    }
}
