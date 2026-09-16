import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallRecoveryCancellationTests: XCTestCase {
    func testCancellationBeforeRecoveryDispatchPreservesJournalAndTemporaryInputs() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        _ = try await f.failFirstFinalization()
        let bytes = try Data(contentsOf: f.paths.journal), inspections = f.recoveryProbe.snapshot.inspections
        let acknowledgement = try XCTUnwrap(f.session.start())
        f.session.cancel(); XCTAssertNil(f.session.start())
        await acknowledgement.value
        XCTAssertEqual(f.session.stage, .cancelled); XCTAssertFalse(f.session.isRunning)
        XCTAssertEqual(f.recoveryProbe.snapshot.inspections, inspections)
        XCTAssertEqual(f.recoveryProbe.snapshot.recoveries, 0)
        try assertPreserved(f, journal: bytes)
    }
    func testCancellationDuringInspectionRetainsBusyUntilWorkerReturnsWithoutRecovery() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        _ = try await f.failFirstFinalization()
        let bytes = try Data(contentsOf: f.paths.journal)
        f.recoveryProbe.arm(.inspection)
        let acknowledgement = try XCTUnwrap(f.session.start())
        let entered = await f.recoveryProbe.waitForEntry(); XCTAssertTrue(entered)
        XCTAssertTrue(Thread.isMainThread)
        f.session.cancel(); XCTAssertTrue(f.session.isRunning); XCTAssertNil(f.session.start())
        XCTAssertEqual(f.recoveryProbe.snapshot.recoveries, 0)
        f.recoveryProbe.release(); await acknowledgement.value
        XCTAssertEqual(f.session.stage, .cancelled); XCTAssertFalse(f.session.isRunning)
        XCTAssertFalse(f.recoveryProbe.snapshot.gateTimedOut)
        XCTAssertTrue(f.recoveryProbe.snapshot.workerThreads.allSatisfy { !$0 })
        try assertPreserved(f, journal: bytes)
    }
    func testCancellationAfterRecoveryBeginsWaitsForActualCommitAndCompletesOnce() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        _ = try await f.failFirstFinalization()
        f.recoveryProbe.arm(.seeding)
        let acknowledgement = try XCTUnwrap(f.session.start())
        await acknowledgement.value
        let pipeline = Task { await f.queue.runNext() }
        let entered = await f.recoveryProbe.waitForEntry(); XCTAssertTrue(entered)
        assertRecoveryReservedAfterCancellation(f)
        // Cancellation of the acknowledgement also cannot abandon an admitted transaction worker.
        acknowledgement.cancel()
        f.recoveryProbe.release(); await pipeline.value
        XCTAssertEqual(f.session.stage, .done); XCTAssertFalse(f.session.isRunning)
        XCTAssertEqual(f.completions, 1); XCTAssertNil(f.session.start())
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalVars), f.originalVars)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertEqual(f.recoveryProbe.snapshot.recoveries, 1); XCTAssertEqual(f.recoveryProbe.snapshot.seeds, 1)
        XCTAssertFalse(f.recoveryProbe.snapshot.gateTimedOut)
        XCTAssertTrue(f.recoveryProbe.snapshot.workerThreads.allSatisfy { !$0 })
        f.assertNoSecondPipeline()
    }
    func testCancellationAfterRecoveryBeginsReportsActualFailureAndRetainsTransaction() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        _ = try await f.failFirstFinalization()
        f.recoveryProbe.arm(.seeding, failSeeder: true)
        let acknowledgement = try XCTUnwrap(f.session.start())
        await acknowledgement.value
        let pipeline = Task { await f.queue.runNext() }
        let entered = await f.recoveryProbe.waitForEntry(); XCTAssertTrue(entered)
        assertRecoveryReservedAfterCancellation(f)
        f.recoveryProbe.release(); await pipeline.value
        guard case .failed(let detail) = f.session.stage else { XCTFail("Recovery failure must not become cancellation"); return }
        XCTAssertTrue(detail.contains("synthetic recovery seed failure"))
        XCTAssertFalse(f.session.isRunning); XCTAssertEqual(f.completions, 0)
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.paths.journal.path))
        XCTAssertEqual(try Data(contentsOf: f.paths.stagedDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.stagedVars), f.originalVars)
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: f.paths.config)).installPending, true)
        XCTAssertFalse(f.recoveryProbe.snapshot.gateTimedOut); f.assertNoSecondPipeline()
    }
    func testPublishedAdmissionCannotStartASecondPipelineReentrantly() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        var attempted = false, refused = false, sawBusy = false
        let subscription = f.session.$stage.dropFirst().sink { stage in
            guard stage == .validating, !attempted else { return }
            attempted = true; sawBusy = f.session.isRunning; refused = f.session.start() == nil
        }
        defer { withExtendedLifetime(subscription) {} }
        _ = try await f.failFirstFinalization()
        XCTAssertTrue(attempted); XCTAssertTrue(sawBusy); XCTAssertTrue(refused)
        f.assertNoSecondPipeline()
    }
    private func assertRecoveryReservedAfterCancellation(_ f: HvfWindowsInstallRecoveryFixture) {
        XCTAssertEqual(f.session.stage, .recovering); XCTAssertTrue(Thread.isMainThread)
        f.session.cancel(); f.session.cancel()
        XCTAssertEqual(f.session.stage, .recovering); XCTAssertTrue(f.session.isRunning)
        XCTAssertNil(f.session.start()); XCTAssertTrue(f.queue.jobs.isEmpty)
        XCTAssertEqual(f.process.cancellations, 0)
        XCTAssertEqual(f.completions, 0)
    }
    private func assertPreserved(_ f: HvfWindowsInstallRecoveryFixture, journal: Data) throws {
        XCTAssertEqual(try Data(contentsOf: f.paths.journal), journal)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpTargetPath)), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpVarsPath)), f.originalVars)
        XCTAssertEqual(f.completions, 0); f.assertNoSecondPipeline()
    }
}
