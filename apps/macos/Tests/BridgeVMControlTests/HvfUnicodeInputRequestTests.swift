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
        XCTAssertEqual(HvfUnicodeInputRequest(text: String(repeating: "a", count: 65_536), now: now)?.command.utf8.count, 87_431)
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
        XCTAssertNil(rejected.consume(lines: ["BVAGENT CMD \(base.command) exit=1"], now: now), "its output says why")
        XCTAssertEqual(rejected.consume(lines: ["BVAGENT END \(base.command)"], now: now), .failed(.guestRejected))
    }

    /// bvagent-input.ps1 reports SendInput's shortfall as `BVINPUT_FAILED <id> <message>`; only "nothing at all"
    /// (the secure desktop) is `notInserted`, every other failure may have inserted a prefix.
    func testAReceiptThatInsertedNothingIsNotInsertedAndAnyOtherFailureIsRejected() throws {
        let base = try makeRequest()
        let label = "TEXTINPUT \(id.uuidString)"
        func failed(_ message: String) -> [String] {
            ["BVAGENT CMD \(label) exit=1", "BVINPUT_FAILED \(id.uuidString) \(message)", "BVAGENT END \(label)"]
        }
        let nothing = "Exception calling \"Insert\" with \"1\" argument(s): \"unicode-input-incomplete inserted=0 requested=2\""
        for (message, outcome) in [
            (nothing, HvfUnicodeInputRequest.Failure.notInserted),
            (nothing + "\r", .notInserted),
            ("Exception calling \"RequireComplete\" with \"2\" argument(s): \"unicode-input-incomplete inserted=0 requested=6\"",
             .notInserted),
            (nothing.replacingOccurrences(of: "inserted=0", with: "inserted=1"), .guestRejected),
            (nothing.replacingOccurrences(of: "inserted=0", with: "inserted=10"), .guestRejected),
            (nothing.replacingOccurrences(of: "requested=2", with: "requested=0"), .guestRejected),
            ("unicode-request-format", .guestRejected),
        ] {
            var request = base
            XCTAssertEqual(request.consume(lines: failed(message), now: now), .failed(outcome), message)
        }
        var split = base
        let lines = failed(nothing)
        XCTAssertNil(split.consume(lines: [lines[0]], now: now))
        XCTAssertEqual(split.consume(lines: Array(lines[1...]), now: now), .failed(.notInserted), "across polls")
        var foreign = base
        XCTAssertNil(foreign.consume(lines: [lines[0], "BVINPUT_FAILED 00000000-0000-0000-0000-000000000000 " + nothing], now: now))
        XCTAssertEqual(foreign.consume(lines: [lines[2]], now: now), .failed(.guestRejected), "another request's receipt")
        var unclaimed = base
        XCTAssertEqual(unclaimed.consume(lines: ["BVINPUT_FAILED \(id.uuidString) " + nothing] + receipt(base), now: now), .inserted,
                       "a failure line outside a failed envelope claims nothing")
    }

    func testRestartInCompletionBatchAndTimeoutCancelInsteadOfAdvancingInput() throws {
        for reset in ["BVAGENT READY guest", "BVAGENT re-READY guest", "BVAGENT SERVICE start", "PSCI SYSTEM_RESET: reboot 1/8"] {
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
