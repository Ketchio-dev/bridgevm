import XCTest
@testable import BridgeVMProductE2E

final class T17OwnedApplicationCleanupTests: XCTestCase {
    func testInterruptedInstallCancelsOwnedWorkerBeforeApplicationExit() throws {
        let fixture = try T17OwnedInstallCleanupFixture()
        defer { fixture.finish() }
        let clean = T17OwnedApplicationCleanup.stop(installationPending: true,
            press: fixture.press, installStage: fixture.stage, systemOffObserved: { false },
            applicationIsRunning: { fixture.applicationRunning },
            terminate: { fixture.applicationRunning = false }, interrupt: { fixture.applicationRunning = false },
            clock: { fixture.now }, pause: fixture.pause)
        XCTAssertTrue(clean)
        XCTAssertEqual(fixture.presses, ["bridgevm.windows.install.cancel"])
        XCTAssertFalse(fixture.installer.isRunning, "Application exit alone must not acknowledge installer teardown")
        XCTAssertTrue(fixture.exists("closed"), "Owning installer must reap its child before terminal stage")
    }

    func testInstallerQueryFailureCannotBeMaskedByApplicationExit() {
        var now = 0.0, running = true
        let clean = T17OwnedApplicationCleanup.stop(installationPending: true,
            press: { _, _ in throw NSError(domain: "AX refusal", code: 1) },
            installStage: { throw NSError(domain: "AX refusal", code: 1) },
            systemOffObserved: { true }, applicationIsRunning: { running },
            terminate: { running = false }, interrupt: {}, clock: { now }, pause: { now += $0 })
        XCTAssertFalse(clean, "No terminal install acknowledgment was observed")
        XCTAssertFalse(running)
        XCTAssertGreaterThanOrEqual(now, 30)
        XCTAssertLessThan(now, 31)
    }

    func testRuntimeStopRetainsSeparateNaturalShutdownObservation() {
        var now = 0.0, running = true
        var presses: [String] = [], systemOffChecks = 0
        let clean = T17OwnedApplicationCleanup.stop(installationPending: false,
            press: { presses.append($0); XCTAssertEqual($1, 2) }, installStage: { XCTFail("runtime has no installer"); return nil },
            systemOffObserved: { systemOffChecks += 1; return false }, applicationIsRunning: { running },
            terminate: { running = false }, interrupt: {}, clock: { now }, pause: { now += $0 })
        XCTAssertTrue(clean, "Cleanup does not promote a missing Windows SYSTEM_OFF")
        XCTAssertEqual(presses, ["bridgevm.windows.runtime.stop"])
        XCTAssertGreaterThan(systemOffChecks, 0)
        XCTAssertGreaterThanOrEqual(now, 30)
    }

    func testOnlyTerminalInstallStagesAcknowledgeCancellation() {
        for stage in ["취소됨", "실패", "완료", "취소 중…", "Windows 무인 설치", "", "other"] {
            var now = 0.0
            let clean = T17OwnedApplicationCleanup.stop(installationPending: true,
                press: { _, _ in }, installStage: { stage }, systemOffObserved: { XCTFail("not a guest shutdown"); return true },
                applicationIsRunning: { false }, terminate: {}, interrupt: {}, clock: { now }, pause: { now += $0 })
            XCTAssertEqual(clean, ["취소됨", "실패", "완료"].contains(stage), stage)
        }
    }

    func testTerminalInstallDoesNotMaskApplicationStillRunning() {
        var now = 0.0, terminations = 0, interrupts = 0
        let clean = T17OwnedApplicationCleanup.stop(installationPending: true,
            press: { _, _ in }, installStage: { "취소됨" }, systemOffObserved: { false },
            applicationIsRunning: { true }, terminate: { terminations += 1 }, interrupt: { interrupts += 1 },
            clock: { now }, pause: { now += $0 })
        XCTAssertFalse(clean)
        XCTAssertEqual(terminations, 1)
        XCTAssertEqual(interrupts, 1)
        XCTAssertGreaterThanOrEqual(now, 15)
        XCTAssertLessThan(now, 16)
    }
}
