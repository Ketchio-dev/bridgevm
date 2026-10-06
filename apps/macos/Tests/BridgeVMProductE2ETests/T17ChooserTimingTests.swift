import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserTimingTests: XCTestCase {
    func testReturnedStageHasMonotonicEntryElapsedAndRemainingBudget() throws {
        var clock = 1.0
        let timing = T17ChooserTiming(start: 0, deadline: 20, now: { clock })
        try timing.run("location-sheet-dismissal") { clock = 4 }
        XCTAssertEqual(timing.lastComplete, "location-sheet-dismissal")
        XCTAssertTrue(timing.snapshot.contains("entry_ms=1000.000,elapsed_ms=3000.000,remaining_ms=16000.000,return=returned"))
    }

    func testThrownStagePreservesBlockerAndDoesNotClaimCompletion() throws {
        var clock = 0.0
        let timing = T17ChooserTiming(start: 0, deadline: 20, now: { clock })
        try timing.run("location-sheet-dismissal") { clock = 1 }
        let original = T17FileChooser.failure("stage=accept-selection; fixture refusal")
        XCTAssertThrowsError(try timing.run("accept-selection") { clock = 2; throw original }) { error in
            XCTAssertEqual(error as? T17Blocker, original)
        }
        XCTAssertEqual(timing.lastComplete, "location-sheet-dismissal")
        XCTAssertTrue(timing.snapshot.contains("return=threw"))
        XCTAssertTrue(timing.attributed(original).detail.hasPrefix(original.detail))
    }

    func testInvalidOrBackwardsReturnClockIsAnExplicitRefusal() {
        for end in [Double.nan, .infinity, -.infinity, -.greatestFiniteMagnitude, -1.0] {
            var clock = 0.0
            let timing = T17ChooserTiming(start: 0, deadline: 20, now: { clock })
            XCTAssertThrowsError(try timing.run("accept-selection") { clock = end }) { error in
                XCTAssertTrue((error as? T17Blocker)?.detail.contains("clock refused") == true)
            }
            XCTAssertNotEqual(timing.clockState, "valid")
            XCTAssertEqual(timing.lastComplete, "none")
            XCTAssertTrue(timing.snapshot.contains("elapsed_ms=unknown"))
        }
    }

    func testInvalidInitialClockDoesNotInvokeAnAction() {
        let timing = T17ChooserTiming(start: 0, deadline: 20, now: { .nan })
        var actions = 0
        XCTAssertThrowsError(try timing.run("open-control") { actions += 1 })
        XCTAssertEqual(actions, 0)
        XCTAssertEqual(timing.clockState, "nonfinite")
    }

    func testUnknownStageCannotRecordOrEchoPrivateData() {
        let timing = T17ChooserTiming(start: 0, deadline: 20, now: { 0 })
        var actions = 0
        XCTAssertThrowsError(try timing.run("/private/secret-name") { actions += 1 }) { error in
            XCTAssertFalse((error as? T17Blocker)?.detail.contains("secret-name") == true)
        }
        XCTAssertEqual(actions, 0)
        XCTAssertFalse(timing.snapshot.contains("secret-name"))
    }

    func testRecordAndOutputCapsPreserveRecentCompletion() throws {
        var clock = 0.0
        let timing = T17ChooserTiming(start: 0, deadline: 20, now: { clock })
        for _ in 0..<40 { try timing.run("set-location") { clock += 0.001 } }
        XCTAssertEqual(timing.records.count, 16)
        XCTAssertEqual(timing.dropped, 24)
        XCTAssertTrue(timing.snapshot.count <= 882)
        XCTAssertEqual(timing.lastComplete, "set-location")
    }

    func testAcceptSelectionWholeCallOvershootIsVisibleBeforeConfirmation() {
        let driver = Driver(); driver.acceptDelay = 20
        XCTAssertThrowsError(try T17FileChooser.choose(path: "/fixture/share", timeout: 20,
            driver: driver, now: { driver.clock }, pause: {})) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.hasPrefix("stage=accept-selection; timed out waiting"))
                XCTAssertTrue(detail.contains("last_complete=location-sheet-dismissal"))
                XCTAssertTrue(detail.contains("accept-selection:entry_ms=0.000,elapsed_ms=20000.000,remaining_ms=0.000"))
            }
        XCTAssertEqual(driver.selectedReads, 0)
    }

    func testLatePressPreservesTheKnownSuccessOrCannotCompleteResult() {
        for result in [AXError.success, .cannotComplete] {
            var clock = 0.0
            XCTAssertThrowsError(try T17ChooserOpenAction.perform(identifier: "fixture", deadline: 20,
                enabled: { true }, press: { clock = 21; return result }, now: { clock }, pause: {})) { error in
                    XCTAssertTrue((error as? T17Blocker)?.detail.contains("ax_error=\(result.rawValue)") == true)
                }
        }
    }

    func testLatePreludeNamesItsReturnedDurationWithoutOpening() {
        var clock = 0.0
        XCTAssertThrowsError(try T17ChooserAdmission.choose(path: "/fixture/share", timeout: 20,
            now: { clock }, pause: {}, lookup: { _ in clock = 21; return 1 },
            driver: { _, _, _ in XCTFail(); return Driver() })) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.contains("target-lookup:entry_ms=0.000,elapsed_ms=21000.000,remaining_ms=-1000.000"))
            }
    }

    private typealias Driver = T17ChooserAdmissionFixture
}
