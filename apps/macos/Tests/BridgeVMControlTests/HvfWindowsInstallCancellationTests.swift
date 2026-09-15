import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallCancellationTests: XCTestCase {
    func testCacheAwaitCancellationPreservesUntouchedMediaAndStartsNoProcess() async throws {
        for verified in [false, true] {
            let fixture = try HvfWindowsInstallCancellationFixture(boundary: .cache, cacheVerified: verified)
            defer { fixture.clean() }
            try await fixture.whilePaused { session in
                fixture.cancelAndAssertReserved(session)
                XCTAssertEqual(fixture.prepares, 0)
                XCTAssertEqual(fixture.process.requests, 0)
                XCTAssertThrowsError(try HvfWindowsInstallSourceLock(sourceImagePath: fixture.plan.sourceImagePath))
            }
            fixture.assertCancelled()
            XCTAssertEqual(fixture.prepares, 0, "Cancellation must not write vars after cache verification returns")
            XCTAssertEqual(fixture.process.requests, 0, "Cancellation must not dispatch source or install work")
            for path in [fixture.plan.sourceImagePath, fixture.plan.tmpTargetPath, fixture.plan.tmpVarsPath] {
                XCTAssertEqual(try? Data(contentsOf: URL(fileURLWithPath: path)), fixture.sentinel)
            }
            _ = try HvfWindowsInstallSourceLock(sourceImagePath: fixture.plan.sourceImagePath)
        }
    }

    func testSuccessfulSourceProcessAcknowledgmentCannotContinueCancelledInstall() async throws {
        let fixture = try HvfWindowsInstallCancellationFixture(boundary: .source, cacheVerified: false)
        defer { fixture.clean() }
        try await fixture.whilePaused { session in
            fixture.cancelAndAssertReserved(session)
            XCTAssertEqual(fixture.prepares, 0)
            XCTAssertEqual(fixture.process.requests, 1)
        }
        fixture.assertCancelled()
        XCTAssertEqual(fixture.prepares, 0)
        XCTAssertEqual(fixture.process.requests, 1)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: fixture.plan.sourceImagePath)), fixture.sentinel)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.plan.tmpTargetPath))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.plan.tmpVarsPath))
    }

    func testSuccessfulInstallProcessAcknowledgmentCannotFinalizeCancelledMedia() async throws {
        let fixture = try HvfWindowsInstallCancellationFixture(boundary: .install, cacheVerified: true)
        defer { fixture.clean() }
        try await fixture.whilePaused { session in
            XCTAssertEqual(fixture.prepares, 1)
            fixture.cancelAndAssertReserved(session)
        }
        fixture.assertCancelled()
        XCTAssertEqual(fixture.process.requests, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.plan.tmpTargetPath))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.plan.tmpVarsPath))
        let retry = try XCTUnwrap(fixture.session.start())
        await retry.value
        XCTAssertEqual(fixture.session.stage, .preparingSource)
        XCTAssertEqual(fixture.process.resets, 2)
        XCTAssertEqual(fixture.queue.jobs.count, 1)
        fixture.session.cancel()
        await fixture.queue.runNext()
        XCTAssertEqual(fixture.session.stage, .cancelled)
        XCTAssertEqual(fixture.process.requests, 1)
    }

    func testCancelDoesNotChangeIdleOrFailedSession() async throws {
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { _ in "owned validation refusal" }, schedule: queue.enqueue)
        session.cancel()
        XCTAssertEqual(session.stage, .idle)
        XCTAssertTrue(session.logLines.isEmpty)
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        let lines = session.logLines
        session.cancel()
        XCTAssertEqual(session.stage, .failed("owned validation refusal"))
        XCTAssertEqual(session.logLines, lines)
        XCTAssertTrue(queue.jobs.isEmpty)
    }

    func testNativeProcessCancellationLatchRefusesLaunchUntilReset() async throws {
        let plan = HvfWindowsInstallTestSupport.plan()
        try FileManager.default.createDirectory(at: plan.repoRoot, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: plan.repoRoot) }
        let marker = plan.repoRoot.appendingPathComponent("owned-process-marker")
        let process = HvfWindowsInstallProcess()
        var lines: [String] = []
        process.cancel()
        let refused = await process.run(plan: plan, arguments: ["/usr/bin/touch", marker.path],
            environment: [:], progressLog: nil, output: { lines.append($0) })
        XCTAssertFalse(refused)
        XCTAssertTrue(lines.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))
        process.reset()
        let accepted = await process.run(plan: plan, arguments: ["/usr/bin/true"],
            environment: [:], progressLog: nil, output: { lines.append($0) })
        XCTAssertTrue(accepted, "An explicit new-attempt reset must admit an ordinary deterministic helper")
        XCTAssertEqual(lines, ["$ /usr/bin/true"])
    }
}
