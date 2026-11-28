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
    func testDiagnosticAssertionNameIsNonemptyASCII() {
        let name = ControlSystemPowerAssertion.diagnosticName
        XCTAssertFalse(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        XCTAssertTrue(name.utf8.allSatisfy { $0 < 128 }, "pmset must print the diagnostic name: \(String(reflecting: name))")
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
        power.update(UUID(), active: true)
        power.update(UUID(), active: true)
        XCTAssertFalse(power.held)
        power.shutdown()
        XCTAssertEqual(api.released, 0)
    }
    /// Sleep and clock audit (finding 4): the failure only went to the log, so the Mac idle-slept
    /// mid-task without a word. The running VM's card says so until the VM stops.
    func testKeepAwakeFailureIsShownOnTheRunningVMUntilItStops() {
        let api = API(); api.succeeds = false
        let power = ControlAppPower(api: api)
        let session = HvfEngineSession(config: HvfEngineView.defaultConfig(), processIsRunning: { _ in true })
        session.sleepPrevention = power
        let runtime = session.sleepPreventionID
        XCTAssertNil(power.keepAwakeNotice(for: runtime))
        session.attachedToExistingProcess = true
        session.connectionState = .booting
        XCTAssertEqual(power.keepAwakeNotice(for: runtime),
            "BridgeVM couldn’t keep your Mac awake, so check its sleep settings before leaving Windows running unattended.")
        XCTAssertNil(power.keepAwakeNotice(for: UUID()), "A VM that isn't running shows nothing")
        session.attachedToExistingProcess = false; session.markStopped()
        XCTAssertNil(power.keepAwakeNotice(for: runtime))
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
    func testRunningVMsAboveTheMacsSpareMemoryExplainSlowdownUntilOneStops() {
        let power = ControlAppPower(api: API())
        var config = HvfEngineView.defaultConfig(); config.ramMiB = 4096
        let sessions = (0..<2).map { _ in HvfEngineSession(config: config, processIsRunning: { _ in true }) }
        for session in sessions {
            session.sleepPrevention = power
            session.attachedToExistingProcess = true
            session.connectionState = .booting
        }
        let first = sessions[0].sleepPreventionID, eightGB: UInt64 = 8 << 30
        XCTAssertEqual(power.sharedMemoryNotice(for: first, hostBytes: eightGB),
            "Your running VMs use 8 GB of memory together, more than this Mac can spare, so Windows and your Mac may slow down. Shut one down, or give the VMs less memory in Settings.")
        XCTAssertNil(power.sharedMemoryNotice(for: first, hostBytes: 16 << 30), "8 GiB fits what a 16 GB Mac can spare")
        sessions[1].attachedToExistingProcess = false; sessions[1].markStopped()
        XCTAssertNil(power.sharedMemoryNotice(for: first, hostBytes: eightGB), "One VM alone stays within its own limit")
        sessions[0].attachedToExistingProcess = false; sessions[0].markStopped()
    }
    /// One VM alone above what the Mac can spare (one made on a Mac with more memory keeps its amount)
    /// said nothing.
    func testOneVMAboveTheMacsSpareMemorySaysSo() {
        let power = ControlAppPower(api: API())
        var config = HvfEngineView.defaultConfig(); config.ramMiB = 16384
        let session = HvfEngineSession(config: config, processIsRunning: { _ in true })
        session.sleepPrevention = power
        session.attachedToExistingProcess = true
        session.connectionState = .booting
        XCTAssertEqual(power.sharedMemoryNotice(for: session.sleepPreventionID, hostBytes: 16 << 30),
            "This VM uses 16 GB of memory, more than this Mac can spare, so Windows and your Mac may slow down. Give it less memory in Settings.")
        XCTAssertNil(power.sharedMemoryNotice(for: session.sleepPreventionID, hostBytes: 32 << 30))
        session.attachedToExistingProcess = false; session.markStopped()
    }
    func testDisabledDefaultDoesNotAcquire() {
        let api = API()
        let power = ControlAppPower(api: api, enabled: false)
        power.update(UUID(), active: true)
        XCTAssertFalse(power.held)
        XCTAssertEqual(api.acquired, 0)
    }
}
