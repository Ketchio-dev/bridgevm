import Foundation
import XCTest

@testable import BridgeVMControl

final class HvfClipboardPasteTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_000)

    func testPasteWaitsForCompleteCorrelatedFrameAcrossPolls() {
        var request = HvfClipboardPaste(base64: "YWJj", now: now)
        XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0"], now: now))
        XCTAssertNil(request.consume(lines: [request.marker + "\r"], now: now))
        XCTAssertEqual(request.consume(lines: ["BVAGENT END \(request.command)"], now: now), true)
        XCTAssertNil(request.consume(lines: ["BVAGENT END \(request.command)"], now: now))
    }

    func testOldRequestAndAutomaticSyncDoNotAcknowledgeNewPaste() {
        let old = HvfClipboardPaste(base64: "YWJj", now: now)
        var fresh = HvfClipboardPaste(base64: "YWJj", now: now)
        XCTAssertNotEqual(old.command, fresh.command)
        XCTAssertNil(fresh.consume(lines: ["BVAGENT CLIPSYNC host->guest bytes=3 t=1",
            "BVAGENT CMD \(old.command) exit=0", old.marker,
            "BVAGENT END \(old.command)"], now: now))
    }

    func testErrorAndMissingMarkerRefusePaste() {
        var failed = HvfClipboardPaste(base64: "YWJj", now: now)
        XCTAssertEqual(failed.consume(lines: ["BVAGENT CMD \(failed.command) exit=1"], now: now), false)
        var missing = HvfClipboardPaste(base64: "YWJj", now: now)
        XCTAssertEqual(missing.consume(lines: ["BVAGENT CMD \(missing.command) exit=0",
            "BVAGENT END \(missing.command)"], now: now), false)
    }

    func testTimeoutAndGuestRestartInvalidatePendingPaste() {
        var expired = HvfClipboardPaste(base64: "YWJj", now: now)
        XCTAssertEqual(expired.consume(lines: [], now: now.addingTimeInterval(30)), false)
        for reset in ["BVAGENT re-READY guest t=1", "BVAGENT READY host=guest t=1",
                      "PSCI SYSTEM_RESET: reboot 1/8", "BVAGENT SERVICE start t=1"] {
            var request = HvfClipboardPaste(base64: "YWJj", now: now)
            XCTAssertEqual(request.consume(lines: [reset], now: now), false)
        }
    }
}
