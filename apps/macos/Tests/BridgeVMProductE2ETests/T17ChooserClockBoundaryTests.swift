import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserClockBoundaryTests: XCTestCase {
    func testCapturedStartCannotBeResetByABackwardsOrInvalidSecondRead() {
        for next in [99.0, Double.nan, .infinity] {
            var first = true, lookups = 0, drivers = 0
            XCTAssertThrowsError(try T17ChooserAdmission.choose(path: "/fixture/share", timeout: 20,
                now: { if first { first = false; return 100 }; return next }, pause: {},
                lookup: { _ in lookups += 1; return 1 },
                driver: { _, _, _ in drivers += 1; return T17ChooserAdmissionFixture() })) { error in
                    XCTAssertTrue((error as? T17Blocker)?.detail.contains("clock=" + (next.isFinite ? "backwards" : "nonfinite")) == true)
                }
            XCTAssertEqual(lookups, 0); XCTAssertEqual(drivers, 0)
        }
    }

    func testEnabledReadClockRefusalNeverPressesAndRemainsSticky() {
        for next in [Double.nan, -.infinity, -1.0] {
            var clock = 0.0, presses = 0
            let timing = T17ChooserTiming(start: 0, deadline: 20, now: { clock })
            XCTAssertThrowsError(try T17ChooserOpenAction.perform(identifier: "fixture", deadline: 20,
                enabled: { clock = next; return true }, press: { presses += 1; return .success },
                now: timing.checkedNow, pause: {}))
            XCTAssertEqual(presses, 0)
            clock = 1
            XCTAssertEqual(timing.checkedNow(), .infinity)
            XCTAssertNotEqual(timing.clockState, "valid")
        }
    }

    func testTimingFooterFitsItsCapAndRetainsTheNewestWholeRecord() throws {
        var clock = 1e100
        let timing = T17ChooserTiming(start: clock, deadline: clock * 2, now: { clock })
        for _ in 0..<20 { try timing.run("set-location") { clock *= 1.001 } }
        try timing.run("accept-selection") { clock *= 1.001 }
        let detail = timing.attributed(T17Blocker(code: "fixture", detail: "")).detail
        XCTAssertTrue(detail.count <= 900)
        XCTAssertTrue(detail.contains("accept-selection:entry_ms=out_of_range"))
        XCTAssertTrue(detail.hasSuffix("return=returned}"))
    }
}
