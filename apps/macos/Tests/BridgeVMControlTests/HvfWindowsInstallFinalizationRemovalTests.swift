import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallFinalizationRemovalTests: XCTestCase {
    private struct InjectedParentSyncFailure: LocalizedError {
        var errorDescription: String? { "injected transaction parent sync failure" }
    }

    func testParentSyncFailureAfterTransactionRemovalDoesNotReopenCompletedInstall() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .configStaged)
        defer { f.clean() }
        let store = HvfWindowsInstallSessionStore(makeSession: { _ in f.session })
        let session = store.session(for: f.config, request: f.plan.request, makePlan: { f.plan })
        let requestBytes = try Data(contentsOf: f.paths.pendingRequest)
        try await f.attempt()
        let failedStage = session.stage, failedLog = session.logLines
        guard case .failed = failedStage else { return XCTFail("Expected interrupted finalization") }
        let interrupted = try HvfWindowsInstallFinalization.loadJournal(f.paths.journal)
        XCTAssertEqual(interrupted.phase, .configStaged)
        XCTAssertEqual(try HvfWindowsInstallFinalization.loadConfig(f.paths.config).installPending, true)
        guard case .pending = try HvfWindowsInstallRecovery.inspect(plan: f.plan) else {
            return XCTFail("Expected the interrupted transaction to be recoverable")
        }

        var syncAttempts = 0
        let first = HvfWindowsInstallFinalization.reconcile(
            config: try HvfWindowsInstallFinalization.loadConfig(f.paths.config),
            libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder,
            removeTransaction: { transaction in
                XCTAssertEqual(transaction, f.paths.transaction)
                let committed = try HvfWindowsInstallFinalization.loadJournal(f.paths.journal)
                XCTAssertEqual(committed.transactionID, interrupted.transactionID)
                XCTAssertEqual(committed.phase, .committed)
                // Use the production removal, including its path checks and real deletion.
                // Only the parent sync fails, after the journal has actually disappeared.
                try HvfWindowsInstallDurability.durableRemove(transaction, syncParent: { parent in
                    XCTAssertEqual(parent, f.paths.metadata)
                    XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
                    XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.journal.path))
                    XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.pendingRequest.path))
                    XCTAssertEqual(try HvfWindowsInstallFinalization.loadConfig(f.paths.config).installPending, false)
                    try self.assertPublishedBytes(f, request: requestBytes)
                    syncAttempts += 1
                    throw InjectedParentSyncFailure()
                })
            })

        XCTAssertEqual(syncAttempts, 1)
        XCTAssertTrue(first.issue?.contains("설치를 다시 실행하지 말고") == true)
        XCTAssertFalse(first.issue?.contains("injected") == true)
        XCTAssertEqual(first.config.installPending, false,
                       "A cleanup sync error must not demote an already published installation")
        let persisted = try HvfWindowsInstallFinalization.loadConfig(f.paths.config)
        XCTAssertEqual(persisted.installPending, false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.journal.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.pendingRequest.path))
        try assertPublishedBytes(f, request: requestBytes)

        // A subsequent library reconciliation reads the persisted config, not the staged one.
        let second = HvfWindowsInstallFinalization.reconcile(
            config: persisted, libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder)
        XCTAssertEqual(second.config.installPending, false)
        XCTAssertEqual(try HvfWindowsInstallFinalization.loadConfig(f.paths.config).installPending, false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.pendingRequest.path),
                       "The done request must not be restored as a fresh-install request")
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.journal.path))
        try assertPublishedBytes(f, request: requestBytes)
        // Inspection may refuse a completed config, but must not authorize fresh installation.
        XCTAssertNotEqual(try? HvfWindowsInstallRecovery.inspect(plan: f.plan), .fresh)

        let retained = try XCTUnwrap(store.record(for: f.config.slug))
        XCTAssertTrue(retained.session === session)
        XCTAssertEqual(retained.sourceConfig, f.config)
        XCTAssertEqual(retained.request, f.plan.request)
        XCTAssertEqual(retained.session.stage, failedStage)
        XCTAssertEqual(retained.session.logLines, failedLog)
        XCTAssertFalse(session.isRunning)
        XCTAssertEqual(f.completions, 0)
        f.assertNoSecondPipeline()
    }

    private func assertPublishedBytes(_ f: HvfWindowsInstallRecoveryFixture, request: Data) throws {
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalVars), f.originalVars)
        XCTAssertEqual(try Data(contentsOf: f.paths.doneRequest), request)
    }
}
