import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfRedactedInputReceiptTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 20_000)
    private let id = UUID(uuidString: "01234567-89AB-CDEF-0123-456789ABCDEF")!

    func testPayloadFreeTextAndKeyReceiptsComplete() throws {
        for event: HvfOrderedInputQueue.Event in [.text("private text"), .key("ctrl+v")] {
            var request = try XCTUnwrap(HvfUnicodeInputRequest(event: event, now: now, id: id))
            let label = request.command.split(separator: " ", maxSplits: 2).prefix(2).joined(separator: " ")
            XCTAssertEqual(request.consume(lines: [
                "BVAGENT CMD \(label) exit=0",
                "BVINPUT_INSERTED \(id.uuidString) \(request.insertedEventCount)",
                "BVAGENT END \(label)"
            ], now: now), .inserted)
        }
    }

    func testMixedLegacyAndRedactedEnvelopesAreRejected() throws {
        let original = try XCTUnwrap(HvfUnicodeInputRequest(text: "private", now: now, id: id))
        let redacted = "TEXTINPUT \(id.uuidString)"
        for (header, end) in [(original.command, redacted), (redacted, original.command)] {
            var request = original
            XCTAssertEqual(request.consume(lines: [
                "BVAGENT CMD \(header) exit=0",
                "BVINPUT_INSERTED \(id.uuidString) \(request.insertedEventCount)",
                "BVAGENT END \(end)"
            ], now: now), .failed(.invalidReceipt))
        }
    }

    func testRedactedEndWithoutHeaderIsRejected() throws {
        var request = try XCTUnwrap(HvfUnicodeInputRequest(text: "private", now: now, id: id))
        XCTAssertEqual(request.consume(lines: ["BVAGENT END TEXTINPUT \(id.uuidString)"],
                                       now: now), .failed(.invalidReceipt))
    }
}
