import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfInputCapabilitiesRequestTests: XCTestCase {
    private let id = UUID(uuidString: "12345678-1234-1234-1234-123456789abc")!
    private var marker: String { "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536" }

    func testRequiresCompleteExactCapabilityReceipt() {
        var request = HvfInputCapabilitiesRequest(id: id)
        XCTAssertEqual(request.command, "INPUTCAPS \(id.uuidString)")
        XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0\r"]))
        XCTAssertNil(request.consume(lines: [marker]))
        XCTAssertEqual(request.consume(lines: ["BVAGENT END \(request.command)"]), .supported)
        XCTAssertNil(request.consume(lines: [marker]))
    }

    /// The probe prints the agent's markers after `| ` (guest_text.rs guest_lines); a printed END ends nothing.
    func testMarkersAfterTheGuestLinePrefixReadTheSame() {
        var request = HvfInputCapabilitiesRequest(id: id)
        XCTAssertEqual(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0", "| " + marker + "\r",
            "| BVAGENT END \(request.command)", "| BVINPUT_TEXT \(id.uuidString) CARET", "| ",
            "BVAGENT END \(request.command)"]), .supported)
        XCTAssertTrue(request.caret)
    }

    func testExtraCapabilityKeywordFromANewerAgentIsAccepted() {
        var request = HvfInputCapabilitiesRequest(id: id)
        XCTAssertEqual(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0",
            "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536 CLIPBOARDINPUT",
            "BVAGENT END \(request.command)"]), .supported)
    }

    func testUnknownVersionMissingKeySupportAndWrongLimitFailClosed() {
        for value in ["2 TEXTINPUT KEYINPUT 65536", "1 TEXTINPUT 65536", "1 TEXTINPUT KEYINPUT 999999"] {
            var request = HvfInputCapabilitiesRequest(id: id)
            XCTAssertEqual(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0",
                "BVINPUT_CAPS \(id.uuidString) \(value)", "BVAGENT END \(request.command)"]),
                .failed(.invalidReceipt))
        }
    }

    func testOldAgentFailureAndMissingMarkerNeverEnableOrderedInput() {
        var rejected = HvfInputCapabilitiesRequest(id: id)
        XCTAssertEqual(rejected.consume(lines: ["BVAGENT CMD \(rejected.command) exit=1"]),
                       .failed(.unsupported))
        var incomplete = HvfInputCapabilitiesRequest(id: id)
        XCTAssertEqual(incomplete.consume(lines: ["BVAGENT CMD \(incomplete.command) exit=0",
            "BVAGENT END \(incomplete.command)"]), .failed(.invalidReceipt))
    }

    func testRestartInvalidatesEarlierSuccessInSameBatch() {
        var request = HvfInputCapabilitiesRequest(id: id)
        XCTAssertEqual(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0", marker,
            "BVAGENT END \(request.command)", "BVAGENT re-READY"]), .failed(.restarted))
    }

    func testForeignAndOutOfOrderReceiptsCannotEnableSupport() {
        var request = HvfInputCapabilitiesRequest(id: id)
        XCTAssertNil(request.consume(lines: ["BVINPUT_CAPS other 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]))
        XCTAssertEqual(request.consume(lines: [marker]), .failed(.invalidReceipt))
    }
}
