import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfClipboardPasteBoundaryTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_000)
    private let restarts = ["BVAGENT READY host=test t=2", "BVAGENT re-READY guest t=2",
                            "PSCI SYSTEM_RESET: reboot 1/8", "BVAGENT SERVICE start t=2"]

    func testRestartAnywhereInBatchOverridesSuccessfulFrame() {
        for restart in restarts {
            for position in 0...3 {
                var request = HvfClipboardPaste(base64: "YWJj", now: now)
                var lines = ["BVAGENT CMD \(request.command) exit=0",
                             request.marker, "BVAGENT END \(request.command)"]
                lines.insert(restart + "\r", at: position)
                XCTAssertEqual(request.consume(lines: lines, now: now), false,
                               "restart at position \(position) must cancel paste")
                XCTAssertNil(request.consume(lines: lines, now: now))
            }
        }
    }

    func testRestartAfterSplitFrameEndCancelsPaste() {
        for restart in restarts {
            var request = HvfClipboardPaste(base64: "YWJj", now: now)
            XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0",
                                                 request.marker], now: now))
            XCTAssertEqual(request.consume(lines: ["BVAGENT END \(request.command)",
                                                   restart], now: now), false)
        }
    }
}
