import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallValidationWorkerTests: XCTestCase {
    func testActualStartValidatorRunsOffMainThread() async throws {
        let probe = HvfWindowsInstallValidationProbe(error: "synthetic validation refusal")
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { probe.validate($0) }, schedule: queue.enqueue)

        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value

        XCTAssertEqual(probe.snapshot.calls, 1)
        XCTAssertEqual(session.stage, .failed("synthetic validation refusal"))
        XCTAssertEqual(queue.jobs.count, 0)
        XCTAssertEqual(probe.snapshot.threadMain, [false])
        XCTAssertNotNil(session.startedAt)
        XCTAssertFalse(session.isRunning)
    }

    func testMainActorProgressesWhileValidationReservesBusyAdmission() async throws {
        let probe = HvfWindowsInstallValidationProbe(gateFirstCall: true)
        let queue = HvfWindowsInstallPipelineQueue()
        defer { probe.release(); queue.discard() }
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { probe.validate($0) }, schedule: queue.enqueue)
        let acknowledgment = try XCTUnwrap(session.start())
        let started = session.startedAt
        XCTAssertEqual(session.stage, .validating)
        XCTAssertTrue(session.isRunning)
        XCTAssertNotNil(started)
        let duplicate = session.start()
        XCTAssertNil(duplicate)

        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered, "injected validation must enter before the bounded wait expires")
        // These main-actor statements execute while the real worker is held at its finite gate.
        XCTAssertTrue(Thread.isMainThread)
        XCTAssertEqual(probe.snapshot.threadMain, [false])
        XCTAssertEqual(probe.snapshot.finished, 0)
        XCTAssertEqual(session.stage, .validating)
        XCTAssertEqual(session.startedAt, started)
        XCTAssertTrue(queue.jobs.isEmpty)
        probe.release()
        await acknowledgment.value
        await duplicate?.value

        XCTAssertFalse(probe.snapshot.gateTimedOut)
        XCTAssertEqual(probe.snapshot.finished, 1)
        XCTAssertEqual(session.stage, .preparingSource)
        XCTAssertEqual(queue.jobs.count, 1)
    }

    func testCancellationBeforeWorkerDispatchSkipsValidationAndPipeline() async throws {
        let probe = HvfWindowsInstallValidationProbe(error: "must not be observed")
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { probe.validate($0) }, schedule: queue.enqueue)
        let acknowledgment = try XCTUnwrap(session.start())
        session.cancel()
        XCTAssertEqual(session.stage, .cancelling)
        XCTAssertTrue(session.isRunning)
        let duplicate = session.start()
        XCTAssertNil(duplicate)
        await acknowledgment.value
        await duplicate?.value

        XCTAssertEqual(probe.snapshot.calls, 0)
        XCTAssertEqual(session.stage, .cancelled)
        XCTAssertFalse(session.isRunning)
        XCTAssertTrue(queue.jobs.isEmpty)
    }

    func testCancellationDuringWorkerWinsOverErrorAndAllowsRetryAfterAcknowledgment() async throws {
        let probe = HvfWindowsInstallValidationProbe(error: "validation refusal", gateFirstCall: true)
        let queue = HvfWindowsInstallPipelineQueue()
        defer { probe.release(); queue.discard() }
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { probe.validate($0) }, schedule: queue.enqueue)
        let acknowledgment = try XCTUnwrap(session.start())
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered, "injected validation must enter before the bounded wait expires")
        session.cancel()
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(session.stage, .cancelling)
        XCTAssertEqual(probe.snapshot.finished, 0)
        let prematureRetry = session.start()
        XCTAssertNil(prematureRetry)
        probe.release()
        await acknowledgment.value
        await prematureRetry?.value

        XCTAssertFalse(probe.snapshot.gateTimedOut)
        XCTAssertEqual(probe.snapshot.calls, 1)
        XCTAssertEqual(session.stage, .cancelled)
        XCTAssertFalse(session.isRunning)
        XCTAssertTrue(queue.jobs.isEmpty)
        probe.setError(nil)
        let retry = try XCTUnwrap(session.start())
        XCTAssertEqual(session.stage, .validating)
        await retry.value
        XCTAssertEqual(probe.snapshot.threadMain, [false, false])
        XCTAssertEqual(probe.snapshot.finished, 2)
        XCTAssertEqual(session.stage, .preparingSource)
        XCTAssertEqual(queue.jobs.count, 1)
    }
}
