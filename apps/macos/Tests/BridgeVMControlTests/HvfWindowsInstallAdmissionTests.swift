import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallAdmissionTests: XCTestCase {
    private final class ScheduledWork {
        var jobs: [@MainActor () async -> Void] = []
        func enqueue(_ work: @escaping @MainActor () async -> Void) { jobs.append(work) }
        func runNext() async { await jobs.removeFirst()() }
    }

    private func plan() -> HvfWindowsInstallPlan {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("install-admission-" + UUID().uuidString)
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        return HvfWindowsInstallPlan(repoRoot: root, libraryRoot: root.appendingPathComponent("library"),
            bundlePath: root.appendingPathComponent("bundle").path, slug: "admission-" + UUID().uuidString,
            request: request)
    }

    func testStartReservesBusyBeforeDispatchAndRefusesRepeatedAdmission() {
        let queue = ScheduledWork()
        defer { queue.jobs.removeAll() }
        var validationCount = 0
        let session = HvfWindowsInstallSession(plan: plan(), validate: { _ in
            validationCount += 1
            return nil
        }, schedule: queue.enqueue)

        session.start()
        let started = session.startedAt
        XCTAssertTrue(session.isRunning)
        session.start()

        XCTAssertEqual(validationCount, 1)
        XCTAssertEqual(queue.jobs.count, 1)
        XCTAssertEqual(session.startedAt, started)
        // Scheduled closures are deliberately never run: no install pipeline.
    }

    func testValidationRefusalSchedulesNothingAndAllowsRetry() {
        let queue = ScheduledWork()
        defer { queue.jobs.removeAll() }
        var refusal: String? = "입력을 확인하세요."
        var validationCount = 0
        let session = HvfWindowsInstallSession(plan: plan(), validate: { _ in
            validationCount += 1
            return refusal
        }, schedule: queue.enqueue)

        session.start()

        XCTAssertEqual(session.stage, .failed("입력을 확인하세요."))
        XCTAssertFalse(session.isRunning)
        XCTAssertNil(session.startedAt)
        XCTAssertTrue(queue.jobs.isEmpty)
        refusal = nil
        session.start()
        XCTAssertTrue(session.isRunning)
        XCTAssertNotNil(session.startedAt)
        XCTAssertEqual(validationCount, 2)
        XCTAssertEqual(queue.jobs.count, 1)
    }

    private func blockedPlan() throws -> HvfWindowsInstallPlan {
        let plan = plan()
        try FileManager.default.createDirectory(at: plan.repoRoot, withIntermediateDirectories: false)
        addTeardownBlock { try? FileManager.default.removeItem(at: plan.repoRoot) }
        // If the cancellation guard regresses, the first pipeline operation
        // cannot create a source directory, so no subprocess can be reached.
        try Data("not a directory".utf8).write(to: plan.libraryRoot, options: .withoutOverwriting)
        return plan
    }

    func testCancellationBeforeDispatchExitsWithoutPipelineOrTemporaryCleanup() async throws {
        let plan = try blockedPlan()
        let queue = ScheduledWork()
        defer { queue.jobs.removeAll() }
        let target = URL(fileURLWithPath: plan.tmpTargetPath)
        let vars = URL(fileURLWithPath: plan.tmpVarsPath)
        for path in [target, vars] {
            try Data([1, 2, 3, 4]).write(to: path, options: .withoutOverwriting)
            addTeardownBlock { try? FileManager.default.removeItem(at: path) }
        }
        let session = HvfWindowsInstallSession(plan: plan, validate: { _ in nil }, schedule: queue.enqueue)
        var completed = false
        session.onCompleted = { completed = true }
        session.start()
        session.cancel()
        session.start()
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(queue.jobs.count, 1)

        await queue.runNext()

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
        let queue = ScheduledWork()
        defer { queue.jobs.removeAll() }
        var validationCount = 0
        let session = HvfWindowsInstallSession(plan: plan, validate: { _ in
            validationCount += 1
            return nil
        }, schedule: queue.enqueue)
        session.start()
        session.cancel()
        session.start()
        XCTAssertEqual(validationCount, 1)
        await queue.runNext()
        session.start()
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(validationCount, 2)
        XCTAssertEqual(queue.jobs.count, 1)
        session.cancel()
        await queue.runNext()
        XCTAssertEqual(session.stage, .failed("설치가 취소되었습니다."))
        XCTAssertFalse(session.isRunning)
    }

}
