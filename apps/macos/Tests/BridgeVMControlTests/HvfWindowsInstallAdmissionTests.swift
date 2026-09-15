import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallAdmissionTests: XCTestCase {
    func testStartReservesBusyBeforeDispatchAndRefusesRepeatedAdmission() async throws {
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let probe = HvfWindowsInstallValidationProbe()
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { probe.validate($0) }, schedule: queue.enqueue)

        let acknowledgment = try XCTUnwrap(session.start())
        let started = session.startedAt
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(session.stage, .validating)
        let duplicate = session.start()
        XCTAssertNil(duplicate)
        await acknowledgment.value
        await duplicate?.value

        XCTAssertEqual(probe.snapshot.calls, 1)
        XCTAssertEqual(queue.jobs.count, 1)
        XCTAssertEqual(session.startedAt, started)
        // Scheduled closures are deliberately never run: no install pipeline.
    }

    func testValidationRefusalSchedulesNothingAndAllowsRetry() async throws {
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let probe = HvfWindowsInstallValidationProbe(error: "입력을 확인하세요.")
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { probe.validate($0) }, schedule: queue.enqueue)

        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value

        XCTAssertEqual(session.stage, .failed("입력을 확인하세요."))
        XCTAssertFalse(session.isRunning)
        XCTAssertNotNil(session.startedAt)
        XCTAssertTrue(queue.jobs.isEmpty)
        probe.setError(nil)
        let retry = try XCTUnwrap(session.start())
        await retry.value
        XCTAssertTrue(session.isRunning)
        XCTAssertNotNil(session.startedAt)
        XCTAssertEqual(probe.snapshot.calls, 2)
        XCTAssertEqual(queue.jobs.count, 1)
    }

    private func blockedPlan() throws -> HvfWindowsInstallPlan {
        let plan = HvfWindowsInstallTestSupport.plan()
        try FileManager.default.createDirectory(at: plan.repoRoot, withIntermediateDirectories: false)
        addTeardownBlock { try? FileManager.default.removeItem(at: plan.repoRoot) }
        // If the cancellation guard regresses, the first pipeline operation
        // cannot create a source directory, so no subprocess can be reached.
        try Data("not a directory".utf8).write(to: plan.libraryRoot, options: .withoutOverwriting)
        return plan
    }

    func testCancellationBeforeDispatchExitsWithoutPipelineOrTemporaryCleanup() async throws {
        let plan = try blockedPlan()
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let target = URL(fileURLWithPath: plan.tmpTargetPath)
        let vars = URL(fileURLWithPath: plan.tmpVarsPath)
        for path in [target, vars] {
            try Data([1, 2, 3, 4]).write(to: path, options: .withoutOverwriting)
            addTeardownBlock { try? FileManager.default.removeItem(at: path) }
        }
        let session = HvfWindowsInstallSession(plan: plan, validate: { _ in nil }, schedule: queue.enqueue)
        var completed = false
        session.onCompleted = { completed = true }
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        session.cancel()
        let duplicate = session.start()
        XCTAssertNil(duplicate)
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(queue.jobs.count, 1)

        await queue.runNext()
        await duplicate?.value

        XCTAssertEqual(session.stage, .failed("설치가 취소되었습니다."))
        XCTAssertFalse(session.isRunning)
        XCTAssertFalse(completed)
        XCTAssertEqual(try Data(contentsOf: target), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: vars), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: plan.libraryRoot), Data("not a directory".utf8))
        XCTAssertFalse(FileManager.default.fileExists(atPath: plan.sourceImagePath + ".lock"))
    }

    func testCancelledAdmissionAllowsRetryOnlyAfterItsScheduledExit() async throws {
        let plan = try blockedPlan()
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let probe = HvfWindowsInstallValidationProbe()
        let session = HvfWindowsInstallSession(plan: plan,
            validate: { probe.validate($0) }, schedule: queue.enqueue)
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        session.cancel()
        let duplicate = session.start()
        XCTAssertNil(duplicate)
        XCTAssertEqual(probe.snapshot.calls, 1)
        await queue.runNext()
        await duplicate?.value
        let retry = try XCTUnwrap(session.start())
        await retry.value
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(probe.snapshot.calls, 2)
        XCTAssertEqual(queue.jobs.count, 1)
        session.cancel()
        await queue.runNext()
        XCTAssertEqual(session.stage, .failed("설치가 취소되었습니다."))
        XCTAssertFalse(session.isRunning)
    }

}
