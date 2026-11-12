import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlAppPowerTests: XCTestCase {
    final class API: ControlPowerAsserting {
        var acquired = 0, released = 0
        var succeeds = true
        func acquire() -> Bool { acquired += 1; return succeeds }
        func release() { released += 1 }
    }
    func testRuntimeAttachmentHoldsAssertionUntilStopped() {
        let power = ControlAppPower(api: API())
        let session = HvfEngineSession(config: HvfEngineView.defaultConfig(), processIsRunning: { _ in false })
        session.sleepPrevention = power
        session.attachedToExistingProcess = true
        XCTAssertTrue(power.held, "A runtime must prevent idle sleep even without a display window")
        session.markStopped()
        XCTAssertFalse(power.held)
    }

    func testStartStopFailureAndTwoVMsShareOneAssertion() {
        let api = API(), first = UUID(), second = UUID()
        let power = ControlAppPower(api: api)
        power.update(first, active: true)
        power.update(second, active: true)
        XCTAssertTrue(power.held)
        XCTAssertEqual(api.acquired, 1)
        power.update(first, active: false)
        XCTAssertEqual(api.released, 0)
        power.update(second, active: false)
        XCTAssertFalse(power.held)
        XCTAssertEqual(api.released, 1)
        power.update(first, active: true)
        power.update(first, active: false) // failed startup cleanup
        XCTAssertEqual(api.released, 2)
        power.update(second, active: true)
        power.shutdown()
        power.shutdown()
        XCTAssertEqual(api.released, 3)
    }
    func testAPIFailureDoesNotPretendAssertionHeldOrReleaseInvalidAssertion() {
        let api = API(); api.succeeds = false
        let power = ControlAppPower(api: api)
        var failures = 0
        power.failure = { failures += 1 }
        power.update(UUID(), active: true)
        XCTAssertFalse(power.held)
        XCTAssertEqual(failures, 1)
        power.shutdown()
        XCTAssertEqual(api.released, 0)
    }
    func testDisabledDefaultDoesNotAcquire() {
        let api = API()
        let power = ControlAppPower(api: api, enabled: false)
        power.update(UUID(), active: true)
        XCTAssertFalse(power.held)
        XCTAssertEqual(api.acquired, 0)
    }
}
