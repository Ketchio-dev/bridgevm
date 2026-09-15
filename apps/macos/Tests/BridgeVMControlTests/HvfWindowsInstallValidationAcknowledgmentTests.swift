import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallValidationAcknowledgmentTests: XCTestCase {
    func testCancellingAcknowledgmentHandleStillSettlesExactlyOnce() async throws {
        let probe = HvfWindowsInstallValidationProbe(gateFirstCall: true)
        let queue = HvfWindowsInstallPipelineQueue()
        defer { probe.release(); queue.discard() }
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { probe.validate($0) }, schedule: queue.enqueue)
        let acknowledgment = try XCTUnwrap(session.start())
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered, "injected validation must enter before the bounded wait expires")
        acknowledgment.cancel() // The handle is an acknowledgment; only session.cancel cancels installation.
        XCTAssertTrue(acknowledgment.isCancelled)
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(session.stage, .validating)
        XCTAssertEqual(probe.snapshot.finished, 0)
        let duplicate = session.start()
        XCTAssertNil(duplicate)
        probe.release()
        await acknowledgment.value
        await acknowledgment.value
        await duplicate?.value

        XCTAssertFalse(probe.snapshot.gateTimedOut)
        XCTAssertEqual(probe.snapshot.threadMain, [false])
        XCTAssertEqual(probe.snapshot.finished, 1)
        XCTAssertEqual(session.stage, .preparingSource)
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(queue.jobs.count, 1)
    }
}
