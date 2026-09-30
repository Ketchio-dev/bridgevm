import XCTest
@testable import BridgeVMProductE2E

final class T17AudioCountersTests: XCTestCase {
    /// A host stats line in B7's exact field order with `expected` typed shutdown statuses.
    static func line(frames: Int = 48000, drops: Int = 0, expected: Int = 3, active: Int = 0) -> String {
        let values = ["frames_rendered": frames, "drops": drops, "format_drops": drops, "callback_errors": expected + active,
                      "callback_active_errors": active, "callback_stopping_errors": expected, "callback_expected_stopping_errors": expected,
                      "callback_unexpected_errors": active, "callback_stopping_enqueue_during_reset": expected]
        return T17AudioCounters.prefix + T17AudioCounters.fields.map { "\($0)=\(values[$0] ?? 0)" }.joined(separator: " ")
    }

    func testR50CountersPassUnderB7Semantics() {
        // The final host report of physical job t17-fddde07f-audio-directory-pilot-r50.
        XCTAssertTrue(T17AudioCounters.passed("hda CoreAudio stats: frames_rendered=206374 drops=0 dropped_bytes=0 format_drops=0 ring_full_drops=0 queue_stop_errors=0 queue_dispose_errors=0 callback_errors=3 callback_active_errors=0 callback_stopping_errors=3 callback_expected_stopping_errors=3 callback_unexpected_errors=0 callback_stopping_invalid_run_state=0 callback_stopping_queue_invalidated=0 callback_stopping_enqueue_during_reset=3 callback_stopping_disposal_pending=0 callback_stopping_unclassified=0"))
        XCTAssertTrue(T17AudioCounters.passed(Self.line()))
        XCTAssertTrue(T17AudioCounters.passed(Self.line(expected: 0)))
    }

    func testUnexpectedDroppedSilentOrUnreconciledCountersFail() {
        XCTAssertFalse(T17AudioCounters.passed(Self.line(active: 1)), "an active callback error is unexpected")
        XCTAssertFalse(T17AudioCounters.passed(Self.line(drops: 2)))
        XCTAssertFalse(T17AudioCounters.passed(Self.line(frames: 0)))
        let good = Self.line()
        for (from, to) in [("queue_stop_errors=0", "queue_stop_errors=1"), ("queue_dispose_errors=0", "queue_dispose_errors=1"),
                           ("callback_errors=3", "callback_errors=4"), ("callback_expected_stopping_errors=3", "callback_expected_stopping_errors=2"),
                           ("callback_stopping_unclassified=0", "callback_stopping_unclassified=1"), ("drops=0", "drops=00x")] {
            XCTAssertFalse(T17AudioCounters.passed(good.replacingOccurrences(of: from, with: to)), to)
        }
    }

    func testOnlyTheExactOrderedFieldSetIsAccepted() {
        let good = Self.line()
        XCTAssertFalse(T17AudioCounters.passed("hda CoreAudio stats: frames_rendered=48000 drops=0 callback_errors=0"), "legacy short line")
        XCTAssertFalse(T17AudioCounters.passed(good + " extra=0"))
        XCTAssertFalse(T17AudioCounters.passed(good.replacingOccurrences(of: "frames_rendered=48000 drops=0", with: "drops=0 frames_rendered=48000")))
        XCTAssertFalse(T17AudioCounters.passed(good.replacingOccurrences(of: " dropped_bytes=0", with: "")))
        XCTAssertFalse(T17AudioCounters.passed(good.replacingOccurrences(of: "frames_rendered=48000", with: "frames_rendered=-48000")))
    }
}
