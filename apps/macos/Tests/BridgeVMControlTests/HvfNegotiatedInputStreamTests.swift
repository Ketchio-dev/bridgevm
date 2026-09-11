import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfNegotiatedInputStreamTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 4_000)

    private func support(_ command: String) -> [String] {
        let id = command.split(separator: " ")[1]
        return ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 1 TEXTINPUT KEYINPUT 65536",
                "BVAGENT END \(command)"]
    }

    private func readyStream() -> HvfNegotiatedInputStream {
        var stream = HvfNegotiatedInputStream()
        var query = ""
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { query = $0; return true }
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: support(query), now: now) { _ in false }
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(stream.state, .ready)
        return stream
    }

    func testNegotiationWaitsForLegacyDrainBeforeAdmission() {
        var stream = HvfNegotiatedInputStream()
        XCTAssertEqual(stream.enqueue(.text("old"), now: now), .legacy)
        var query = ""
        _ = stream.poll(serviceReady: true, legacyQuiescent: false, lines: [], now: now) { query = $0; return true }
        XCTAssertTrue(query.hasPrefix("INPUTCAPS "))
        _ = stream.poll(serviceReady: true, legacyQuiescent: false, lines: support(query), now: now) { _ in false }
        XCTAssertEqual(stream.state, .waitingForLegacy)
        _ = stream.poll(serviceReady: true, legacyQuiescent: false, lines: [], now: now) { _ in false }
        XCTAssertEqual(stream.enqueue(.text("still old"), now: now), .legacy)
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(stream.enqueue(.text("new"), now: now), .queued)
        XCTAssertEqual(stream.count, 1)
    }

    func testUnsupportedOrSilentAgentAllowsLegacyButDoesNotSendInput() {
        for silent in [false, true] {
            var stream = HvfNegotiatedInputStream()
            var query = ""
            _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { query = $0; return true }
            let lines = silent ? [] : ["BVAGENT CMD \(query) exit=1"]
            _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: lines,
                            now: now.addingTimeInterval(silent ? 30 : 0)) { _ in XCTFail("unexpected send"); return false }
            XCTAssertEqual(stream.state, .unavailable)
            XCTAssertEqual(stream.enqueue(.text("legacy"), now: now), .legacy)
        }
    }

    func testFailureAfterDispatchCannotFallBackOrReplay() {
        var stream = readyStream()
        XCTAssertEqual(stream.enqueue(.text("a"), now: now), .queued)
        XCTAssertEqual(stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { _ in false },
                       .cancelled(.transportFailed, discarded: 1))
        XCTAssertEqual(stream.state, .failed)
        XCTAssertEqual(stream.enqueue(.text("b"), now: now), .refused)
        XCTAssertNil(stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) {
            _ in XCTFail("replayed failed input"); return true
        })
    }

    func testFullOrInvalidQueueNeverFallsBack() {
        var stream = readyStream()
        XCTAssertEqual(stream.enqueue(.key("ctrl+alt+delete"), now: now), .refused)
        for _ in 0..<64 { XCTAssertEqual(stream.enqueue(.text("a"), now: now), .queued) }
        XCTAssertEqual(stream.enqueue(.text("overflow"), now: now), .refused)
    }

    func testServiceLossAndRestartClearSupportAndQueuedInput() {
        var stream = readyStream()
        XCTAssertEqual(stream.enqueue(.key("enter"), now: now), .queued)
        XCTAssertEqual(stream.poll(serviceReady: false, legacyQuiescent: true, lines: [], now: now) { _ in false },
                       .cancelled(.sessionChanged, discarded: 1))
        XCTAssertEqual(stream.state, .disconnected)
        var query = ""
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { query = $0; return true }
        _ = stream.poll(serviceReady: true, legacyQuiescent: true,
                        lines: support(query) + ["BVAGENT re-READY"], now: now) { _ in false }
        XCTAssertEqual(stream.state, .disconnected)
    }

    func testLegacyActivityAfterActivationFailsClosed() {
        var stream = readyStream()
        XCTAssertEqual(stream.enqueue(.key("enter"), now: now), .queued)
        XCTAssertEqual(stream.poll(serviceReady: true, legacyQuiescent: false, lines: [], now: now) {
            _ in XCTFail("mixed legacy and acknowledged transports"); return true
        }, .cancelled(.transportFailed, discarded: 1))
        XCTAssertEqual(stream.enqueue(.text("next"), now: now), .refused)
    }
}
