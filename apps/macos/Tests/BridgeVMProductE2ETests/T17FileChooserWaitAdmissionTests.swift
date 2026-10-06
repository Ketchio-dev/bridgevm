import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserWaitAdmissionTests: XCTestCase {
    func testExpiredWaitDoesNotReadOrPause() {
        var reads = 0, pauses = 0
        XCTAssertThrowsError(try T17FileChooserWait.until(stage: "owned fixture", deadline: 1,
            failureContext: { "" }, now: { 1 }, pause: { pauses += 1 }, ready: { reads += 1; return true }))
        XCTAssertEqual(reads, 0); XCTAssertEqual(pauses, 0)
    }
    func testPauseCrossingDeadlineDoesNotReadAgain() {
        var time = 0.0, reads = 0, pauses = 0
        XCTAssertThrowsError(try T17FileChooserWait.until(stage: "owned fixture", deadline: 1,
            failureContext: { "" }, now: { time }, pause: { pauses += 1; time = 1 },
            ready: { reads += 1; return false }))
        XCTAssertEqual(reads, 1); XCTAssertEqual(pauses, 1)
    }
    func testNonfiniteClockDoesNotRead() {
        XCTAssertThrowsError(try T17FileChooserWait.until(stage: "owned fixture", deadline: 1,
            failureContext: { "" }, now: { .nan }, pause: { XCTFail("late pause") },
            ready: { XCTFail("nonfinite read"); return false }))
    }
}
