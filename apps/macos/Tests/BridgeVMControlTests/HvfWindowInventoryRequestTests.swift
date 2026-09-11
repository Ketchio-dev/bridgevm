import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowInventoryRequestTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1000)
    private func row(_ command: String, id: Int = 1) -> String {
        "BVAGENT \(command) WIN \(id) 7 -10 20 640 480 QQ=="
    }

    func testOnlyCompleteMatchingRequestPublishesAndOnlyOnce() throws {
        var request = HvfWindowInventoryRequest(now: now)
        XCTAssertNil(request.consume(lines: [row(request.command)], now: now))
        XCTAssertNil(request.consume(lines: ["BVAGENT WINLIST WINEND",
            "BVAGENT WINLIST \(UUID().uuidString) WINEND"], now: now))
        let result = try XCTUnwrap(request.consume(
            lines: ["BVAGENT \(request.command) WINEND\r"], now: now))
        XCTAssertEqual(try result.get().map(\.id), ["1"])
        XCTAssertNil(request.consume(lines: ["BVAGENT \(request.command) WINEND"], now: now))
    }

    func testEmptyCompleteInventoryIsDifferentFromNoReply() throws {
        var request = HvfWindowInventoryRequest(now: now)
        XCTAssertNil(request.consume(lines: [], now: now))
        let result = try XCTUnwrap(request.consume(
            lines: ["BVAGENT \(request.command) WINEND"], now: now))
        XCTAssertEqual(try result.get().count, 0)
    }

    func testMalformedDuplicateAndGuestErrorFailWithoutPublishingPartialList() {
        for failure in [HvfWindowInventoryError.malformedRecord, .duplicateHandle, .guestError] {
            var request = HvfWindowInventoryRequest(now: now)
            XCTAssertNil(request.consume(lines: [row(request.command)], now: now))
            let payload: String
            switch failure {
            case .duplicateHandle: payload = row(request.command)
            case .guestError: payload = "BVAGENT \(request.command) -> ERR WINLIST failed"
            default: payload = "BVAGENT \(request.command) malformed=WIN broken"
            }
            let result = request.consume(lines: [payload], now: now)
            if case let .failure(error) = result { XCTAssertEqual(error, failure) }
            else { XCTFail("expected explicit failure") }
        }
    }

    func testTimeoutAndRestartInvalidatePendingInventory() {
        for marker in ["BVAGENT READY host=test", "BVAGENT re-READY t=2",
                       "BVAGENT SERVICE start t=2", "PSCI SYSTEM_RESET: requested"] {
            var request = HvfWindowInventoryRequest(now: now)
            if case .failure(.restarted) = request.consume(lines: [marker], now: now) {}
            else { XCTFail("restart did not invalidate inventory") }
        }
        var request = HvfWindowInventoryRequest(now: now)
        if case .failure(.expired) = request.consume(lines: [], now: now.addingTimeInterval(30)) {}
        else { XCTFail("deadline must fail closed") }
    }

    func testRecordAndByteLimitsFailExplicitly() {
        var request = HvfWindowInventoryRequest(now: now)
        let rows = (1...HvfWindowInventoryRequest.maximumRecords).map { row(request.command, id: $0) }
        XCTAssertNil(request.consume(lines: rows, now: now))
        if case .failure(.resourceLimit) = request.consume(
            lines: [row(request.command, id: 4097)], now: now) {}
        else { XCTFail("record limit not enforced") }
        var oversized = HvfWindowInventoryRequest(now: now)
        let line = "BVAGENT \(oversized.command) " + String(repeating: "x", count: HvfWindowInventoryRequest.maximumBytes)
        if case .failure(.resourceLimit) = oversized.consume(lines: [line], now: now) {}
        else { XCTFail("byte limit not enforced") }
    }
}
