import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfUnicodeInputRequestTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 20_000)
    private let id = UUID(uuidString: "01234567-89AB-CDEF-0123-456789ABCDEF")!

    func testEncodesTextAndCountsUTF16EventsRatherThanCharacters() throws {
        let text = "\u{d55c}\u{1f642}e\u{301}"
        let request = try XCTUnwrap(HvfUnicodeInputRequest(text: text, now: now, id: id))
        XCTAssertEqual(request.command, "TEXTINPUT \(id.uuidString) \(Data(text.utf8).base64EncodedString())")
        XCTAssertEqual(request.insertedEventCount, 10)
        XCTAssertNil(HvfUnicodeInputRequest(text: "", now: now))
        XCTAssertNil(HvfUnicodeInputRequest(text: String(repeating: "a", count: 65_537), now: now))
        XCTAssertNotNil(HvfUnicodeInputRequest(text: String(repeating: "a", count: 65_536), now: now))
        XCTAssertNil(HvfUnicodeInputRequest(text: String(repeating: "\u{d55c}", count: 21_846), now: now))
    }

    func testRequiresHeaderMarkerAndEndAcrossPollsAndCompletesOnlyOnce() throws {
        var request = try makeRequest()
        let lines = receipt(request)
        XCTAssertNil(request.consume(lines: [lines[0]], now: now))
        XCTAssertNil(request.consume(lines: [lines[1] + "\r"], now: now))
        XCTAssertEqual(request.consume(lines: [lines[2]], now: now), .inserted)
        XCTAssertNil(request.consume(lines: lines, now: now))
    }

    func testIgnoresAnotherRequestsEnvelopeWithoutAcknowledgingOurInput() throws {
        var request = try makeRequest()
        let other = try XCTUnwrap(HvfUnicodeInputRequest(text: "\u{d55c}", now: now))
        let wrong = ["BVAGENT CMD \(other.command) exit=0", "BVINPUT_INSERTED other 2", "BVAGENT END \(other.command)"]
        XCTAssertNil(request.consume(lines: wrong, now: now))
        XCTAssertEqual(request.consume(lines: receipt(request), now: now), .inserted)
    }

    func testRejectsNonzeroExitPartialCountsMissingAndDuplicateMarkers() throws {
        let base = try makeRequest()
        let lines = receipt(base)
        let invalid: [[String]] = [
            [lines[0], "BVINPUT_INSERTED \(id.uuidString) 1", lines[2]],
            [lines[0], lines[2]], [lines[1], lines[0], lines[2]],
            [lines[0], lines[1], lines[1], lines[2]], [lines[0], lines[0], lines[1], lines[2]]
        ]
        for input in invalid {
            var request = base
            XCTAssertEqual(request.consume(lines: input, now: now), .failed(.invalidReceipt))
        }
        var rejected = base
        XCTAssertEqual(rejected.consume(lines: ["BVAGENT CMD \(base.command) exit=1"], now: now), .failed(.guestRejected))
    }

    func testRestartInCompletionBatchAndTimeoutCancelInsteadOfAdvancingInput() throws {
        for reset in ["BVAGENT READY guest", "BVAGENT re-READY guest", "BVAGENT SERVICE start", "PSCI_SYSTEM_RESET"] {
            var request = try makeRequest()
            XCTAssertEqual(request.consume(lines: receipt(request) + [reset], now: now), .failed(.restarted))
        }
        var request = try makeRequest()
        XCTAssertEqual(request.consume(lines: receipt(request), now: now.addingTimeInterval(30)), .failed(.expired))
        XCTAssertNil(request.consume(lines: [], now: now.addingTimeInterval(31)))
    }

    private func makeRequest() throws -> HvfUnicodeInputRequest {
        try XCTUnwrap(HvfUnicodeInputRequest(text: "\u{d55c}", now: now, id: id))
    }
    private func receipt(_ request: HvfUnicodeInputRequest) -> [String] {
        ["BVAGENT CMD \(request.command) exit=0", "BVINPUT_INSERTED \(id.uuidString) \(request.insertedEventCount)",
         "BVAGENT END \(request.command)"]
    }
}
