import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallRecoveryBoundaryTests: XCTestCase {
    func testRemovedCompletedTransactionIsNotRecreatedByTicketedRecovery() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared)
        defer { f.clean() }
        _ = try await f.failFirstFinalization()
        guard case let .pending(ticket) = try HvfWindowsInstallRecovery.inspect(plan: f.plan) else {
            XCTFail("Expected a pending transaction"); return
        }
        let result = HvfWindowsInstallFinalization.reconcile(config: f.config, libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder)
        XCTAssertNil(result.issue)
        XCTAssertEqual(result.config.installPending, false)
        XCTAssertThrowsError(try HvfWindowsInstallRecovery.recover(plan: f.plan, ticket: ticket,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalVars), f.originalVars)
    }

    func testInstallingPublicationRechecksNewJournalBeforePreparingMedia() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared)
        defer { f.clean() }
        var inserted = false
        var insertionError: Error?
        let subscription = f.session.$stage.dropFirst().sink { stage in
            guard stage == .installing, !inserted else { return }
            inserted = true
            do { try f.createInterruptedTransaction() } catch { insertionError = error }
        }
        defer { withExtendedLifetime(subscription) {} }
        try await f.attempt()
        XCTAssertTrue(inserted)
        XCTAssertNil(insertionError)
        XCTAssertEqual(f.session.stage, .done)
        XCTAssertEqual(f.prepares, 0)
        XCTAssertEqual(f.process.requests, 0)
        XCTAssertEqual(f.completions, 1)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalVars), f.originalVars)
    }

    func testCancellationCleanupPreservesNewJournalOwnedInputs() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared)
        defer { f.clean() }
        weak var activeSession: HvfWindowsInstallSession?
        var journalBytes: Data?
        let execution = HvfWindowsInstallExecution(process: f.process, verifySource: { _ in true },
            prepareMedia: { _ in
                try f.createInterruptedTransaction()
                journalBytes = try Data(contentsOf: f.paths.journal)
                activeSession?.cancel()
            }, finalize: { _ in XCTFail("Cancelled installation cannot finalize") })
        let session = HvfWindowsInstallSession(plan: f.plan, validate: { _ in nil },
            schedule: f.queue.enqueue, execution: execution)
        activeSession = session
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        await f.queue.runNext()
        XCTAssertEqual(session.stage, .cancelled)
        XCTAssertFalse(session.isRunning)
        XCTAssertEqual(f.process.requests, 0)
        XCTAssertEqual(try Data(contentsOf: f.paths.journal), try XCTUnwrap(journalBytes))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpTargetPath)), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpVarsPath)), f.originalVars)
    }

    func testAdmissionCallbackCannotReenterOrSeeItsOwnRunningConflict() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared)
        defer { f.clean() }
        var calls = 0
        f.session.workAdmission = { _ in
            calls += 1
            XCTAssertFalse(f.session.isRunning)
            XCTAssertNil(f.session.start())
            return nil
        }
        try await f.attempt()
        XCTAssertEqual(calls, 1)
        XCTAssertEqual(f.process.requests, 1)
    }
}
