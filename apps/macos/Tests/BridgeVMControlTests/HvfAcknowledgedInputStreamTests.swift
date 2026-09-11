import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfAcknowledgedInputStreamTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)

    private func receipt(_ command: String, count: Int) -> [String] {
        let id = command.split(separator: " ")[1]
        return ["BVAGENT CMD \(command) exit=0", "BVINPUT_INSERTED \(id) \(count)", "BVAGENT END \(command)"]
    }

    func testTextThenEnterIsSentOnlyAfterExactTextReceipt() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.text("\u{D55C}"), now: now))
        XCTAssertTrue(stream.enqueue(.key("enter"), now: now))
        var sent: [String] = []
        let first = stream.advance(lines: [], now: now) { sent.append($0); return true }
        guard case let .sent(id) = first else { return XCTFail("first request not sent") }
        XCTAssertEqual(sent.count, 1)
        XCTAssertTrue(sent[0].hasPrefix("TEXTINPUT "))
        XCTAssertEqual(stream.advance(lines: [], now: now) { _ in XCTFail("duplicate send"); return true }, .waiting)
        XCTAssertEqual(stream.advance(lines: receipt(sent[0], count: 2), now: now) {
            _ in XCTFail("next request sent inside receipt batch"); return true
        }, .inserted(id))
        guard case .sent = stream.advance(lines: [], now: now, send: { sent.append($0); return true }) else {
            return XCTFail("key request not sent")
        }
        XCTAssertTrue(sent[1].hasPrefix("KEYINPUT "))
        XCTAssertEqual(stream.count, 1)
    }

    func testPartialInsertionCancelsAllWithoutReplay() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.key("ctrl+v"), now: now))
        XCTAssertTrue(stream.enqueue(.text("next"), now: now))
        var command = ""
        _ = stream.advance(lines: [], now: now) { command = $0; return true }
        XCTAssertEqual(stream.advance(lines: receipt(command, count: 1), now: now) { _ in false },
                       .cancelled(.transportFailed, discarded: 2))
        XCTAssertEqual(stream.count, 0)
        XCTAssertEqual(stream.advance(lines: receipt(command, count: 4), now: now) {
            _ in XCTFail("replayed failed batch"); return true
        }, .idle)
    }

    func testTransportFailureAndExpiryCancelQueuedEvents() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.text("a"), now: now))
        XCTAssertEqual(stream.advance(lines: [], now: now) { _ in false },
                       .cancelled(.transportFailed, discarded: 1))
        XCTAssertTrue(stream.enqueue(.text("b"), now: now))
        XCTAssertEqual(stream.advance(lines: [], now: now.addingTimeInterval(30)) {
            _ in XCTFail("expired event sent"); return true
        }, .cancelled(.expired, discarded: 1))
    }

    func testRestartWinsOverEarlierSuccessfulReceipt() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.text("a"), now: now))
        var command = ""
        _ = stream.advance(lines: [], now: now) { command = $0; return true }
        XCTAssertEqual(stream.advance(lines: receipt(command, count: 2) + ["BVAGENT READY"], now: now) { _ in false },
                       .cancelled(.sessionChanged, discarded: 1))
        XCTAssertTrue(stream.enqueue(.key("enter"), now: now))
        XCTAssertEqual(stream.advance(lines: ["PSCI_SYSTEM_RESET"], now: now) {
            _ in XCTFail("sent across restart"); return true
        }, .cancelled(.sessionChanged, discarded: 1))
    }

    func testTargetChangeAndOldReceiptDoNotCompleteNewRequest() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.text("old"), now: now))
        var old = ""
        _ = stream.advance(lines: [], now: now) { old = $0; return true }
        XCTAssertEqual(stream.cancel(.targetChanged), .cancelled(.targetChanged, discarded: 1))
        XCTAssertTrue(stream.enqueue(.text("new"), now: now))
        var fresh = ""
        _ = stream.advance(lines: receipt(old, count: 6), now: now) { fresh = $0; return true }
        XCTAssertNotEqual(old, fresh)
        XCTAssertEqual(stream.advance(lines: receipt(old, count: 6), now: now) { _ in false }, .waiting)
    }

    func testInvalidKeysAreRejectedBeforeQueueAdmission() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertFalse(stream.enqueue(.key("ctrl+alt+delete"), now: now))
        XCTAssertFalse(stream.enqueue(.text(""), now: now))
        XCTAssertEqual(stream.count, 0)
    }
}
