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
    func testOptionalPowerFailureNeverPresentsModalAlert() throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().appendingPathComponent("Sources/BridgeVMControl/ControlAppDelegate.swift")
        let source = try String(contentsOf: root)
        let callback = try XCTUnwrap(source.components(separatedBy: "ControlAppPower.shared.failure = {").last?
            .components(separatedBy: "#if DEBUG").first)
        XCTAssertFalse(callback.contains("runModal"), "Optional sleep prevention must not block runtime startup")
    }
    func testPauseReleasesAndResumeReacquires() {
        let power = ControlAppPower(api: API())
        let session = HvfEngineSession(config: HvfEngineView.defaultConfig(), processIsRunning: { _ in true })
        session.sleepPrevention = power
        session.attachedToExistingProcess = true
        session.connectionState = .booting
        XCTAssertTrue(power.held)
        session.pauseReported("BVPAUSE paused")
        XCTAssertFalse(power.held, "Paused Windows must allow idle sleep")
        session.pauseReported("BVPAUSE resumed")
        XCTAssertTrue(power.held)
        session.attachedToExistingProcess = false
        session.markStopped()
    }
    func testPreferenceTakesEffectForActiveRuntimeImmediately() {
        let power = ControlAppPower(api: API()), id = UUID()
        power.update(id, active: true)
        power.enabled = false
        XCTAssertFalse(power.held, "Disabling sleep prevention must release immediately")
        power.enabled = true
        XCTAssertTrue(power.held)
        power.shutdown()
    }
    func testRuntimeAttachmentHoldsAssertionUntilStopped() {
        let power = ControlAppPower(api: API())
        let session = HvfEngineSession(config: HvfEngineView.defaultConfig(), processIsRunning: { _ in false })
        session.sleepPrevention = power
        session.attachedToExistingProcess = true
        session.connectionState = .booting
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
        power.update(UUID(), active: true)
        XCTAssertFalse(power.held)
        XCTAssertEqual(failures, 1, "Optional power failure is reported once without blocking start")
        power.shutdown()
        XCTAssertEqual(api.released, 0)
    }
    func testRealSessionExitHandlingReleasesForGuestExitRunnerDeathAndForceStop() async throws {
        for kind in ["guest", "runner", "force"] {
            let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
            let session = f.base.session(), power = ControlAppPower(api: API())
            session.sleepPrevention = power
            session.process = f.process; session.ownedController = f.controller
            session.ownedProcessIdentity = f.controller.identity
            f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: f.controller.identity.token) }
            defer { f.controller.onChange = nil; power.shutdown() }
            session.connectionState = .booting
            f.controller.receive(.message(try f.event("ready", sequence: 1)))
            XCTAssertTrue(power.held)
            if kind == "guest" {
                f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
            } else {
                f.controller.receive(.failed(.channelClosed))
                if kind == "force" {
                    XCTAssertEqual(session.requestForceStop(try XCTUnwrap(session.forceStopTarget)), .requested)
                }
            }
            try await f.exit(); f.controller.receive(.eof)
            session.poll()
            XCTAssertFalse(power.held, "Exited runner must release sleep prevention: \(kind)")
            if kind != "guest" { XCTAssertNotNil(session.process, "Keep unconfirmed cleanup ownership, not sleep prevention") }
        }
    }
    func testRealFailedStartupReleasesAssertion() async throws {
        let f = try HvfGUIStartFixture(encrypted: true, rejectLaunch: true)
        let power = ControlAppPower(api: API()); f.session.sleepPrevention = power
        do {
            let ticket = try f.start()
            XCTAssertTrue(power.held)
            try await f.wait { !ticket.workerPending }
            XCTAssertFalse(power.held, "Failed start must release sleep prevention")
        } catch { await f.clean(); throw error }
        await f.clean()
    }
    func testDisabledDefaultDoesNotAcquire() {
        let api = API()
        let power = ControlAppPower(api: api, enabled: false)
        power.update(UUID(), active: true)
        XCTAssertFalse(power.held)
        XCTAssertEqual(api.acquired, 0)
    }
}
