import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallRecoveryTests: XCTestCase {
    func testSameSessionRetryRecoversPreparedJournalWithoutReplacingSealedMedia() async throws {
        try await assertRetryRecovers(.prepared)
    }
    func testSameSessionRetryRecoversDiskStagedJournalWithoutReplacingSealedMedia() async throws {
        try await assertRetryRecovers(.diskStaged)
    }
    func testMalformedJournalRefusesFreshEffects() async throws {
        try await assertRefused { f in try Data("{invalid".utf8).write(to: f.paths.journal) }
    }
    func testLegacyJournalRefusesFreshEffects() async throws {
        try await assertRefused { f in
            var journal = try JSONDecoder().decode(HvfWindowsInstallFinalizationJournal.self, from: Data(contentsOf: f.paths.journal))
            journal.schemaVersion = 1
            try JSONEncoder().encode(journal).write(to: f.paths.journal)
        }
    }
    func testDanglingJournalSymlinkRefusesFreshEffects() async throws {
        try await assertRefused { f in
            try FileManager.default.removeItem(at: f.paths.journal)
            try FileManager.default.createSymbolicLink(at: f.paths.journal,
                withDestinationURL: f.root.appendingPathComponent("missing-journal"))
        }
    }
    func testTransactionWithoutJournalRefusesFreshEffects() async throws {
        try await assertRefused { try FileManager.default.removeItem(at: $0.paths.journal) }
    }
    func testChangedPreservedRequestRefusesFreshEffects() async throws {
        try await assertRefused { f in
            var request = f.plan.request; request.diskGiB += 1
            XCTAssertTrue(request.save(bundlePath: f.plan.bundlePath))
        }
    }
    func testBusyRecoveryRefusesFreshEffectsAndPreservesSealedInputs() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        _ = try await f.failFirstFinalization()
        let bytes = try Data(contentsOf: f.paths.journal)
        let lock = try HvfWindowsInstallDurability.TransactionLock(url: f.paths.lock, nonBlocking: true)
        defer { withExtendedLifetime(lock) {} }
        try await f.attempt()
        assertFailed(f); f.assertNoSecondPipeline()
        XCTAssertEqual(try Data(contentsOf: f.paths.journal), bytes)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpVarsPath)), f.originalVars)
        XCTAssertEqual(f.completions, 0)
    }
    func testIndependentReconciliationCannotRestartRetainedFailedSession() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        _ = try await f.failFirstFinalization()
        let result = HvfWindowsInstallFinalization.reconcile(config: f.config, libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder)
        XCTAssertNil(result.issue); XCTAssertEqual(result.config.installPending, false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        try await f.attempt()
        assertFailed(f); f.assertNoSecondPipeline()
        XCTAssertEqual(f.completions, 0)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalVars), f.originalVars)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
    }
    func testQueuedPipelineRecoversTransactionThatAppearedAfterAdmission() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        let acknowledgement = try XCTUnwrap(f.session.start()); await acknowledgement.value
        XCTAssertEqual(f.queue.jobs.count, 1)
        try f.createInterruptedTransaction()
        await f.queue.runNext()
        assertRecoveryWithoutFreshEffects(f)
    }
    func testTransactionAppearingDuringValidationIsRecoveredBeforeFreshEffects() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared, gateValidation: true); defer { f.clean() }
        let acknowledgement = try XCTUnwrap(f.session.start())
        let entered = await f.validator.waitForEntry(); XCTAssertTrue(entered)
        try f.createInterruptedTransaction()
        f.validator.release(); await acknowledgement.value
        if !f.queue.jobs.isEmpty { await f.queue.runNext() }
        XCTAssertFalse(f.validator.snapshot.gateTimedOut)
        assertRecoveryWithoutFreshEffects(f)
    }
    private func assertRecoveryWithoutFreshEffects(_ f: HvfWindowsInstallRecoveryFixture) {
        XCTAssertEqual(f.session.stage, .done); XCTAssertEqual(f.completions, 1)
        XCTAssertEqual(f.validator.snapshot.calls, 1); XCTAssertEqual(f.cacheChecks, 0)
        XCTAssertEqual(f.prepares, 0); XCTAssertEqual(f.process.requests, 0)
        XCTAssertEqual(f.recoveryProbe.snapshot.recoveries, 1)
    }
    private func assertRefused(_ mutate: (HvfWindowsInstallRecoveryFixture) throws -> Void) async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .prepared); defer { f.clean() }
        _ = try await f.failFirstFinalization(); try mutate(f)
        try await f.attempt()
        assertFailed(f); f.assertNoSecondPipeline()
        XCTAssertEqual(f.completions, 0)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpTargetPath)), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpVarsPath)), f.originalVars)
    }
    private func assertFailed(_ f: HvfWindowsInstallRecoveryFixture) {
        guard case .failed = f.session.stage else { XCTFail("Expected refusal, got \(f.session.stage)"); return }
        XCTAssertFalse(f.session.isRunning)
    }
    private func assertRetryRecovers(_ boundary: HvfWindowsInstallFinalizationBoundary) async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: boundary); defer { f.clean() }
        let session = f.session
        let sealed = try await f.failFirstFinalization()
        let journalBytes = try Data(contentsOf: f.paths.journal)
        try await f.attempt() // Same object; no library scan/reload that could recover the journal for us.
        XCTAssertTrue(f.session === session)
        XCTAssertEqual(f.validator.snapshot.calls, 1, "Retry must not re-enter full ISO/input validation")
        XCTAssertEqual(f.cacheChecks, 1, "Retry must not restart source-cache work")
        XCTAssertEqual(f.prepares, 1, "Retry must not overwrite the vars sealed by the original journal")
        XCTAssertEqual(f.process.requests, 1, "Retry must not run another installer against sealed tmp media")
        XCTAssertEqual(session.stage, .done, "A valid durable finalization should resume before fresh installation effects")
        XCTAssertEqual(f.completions, 1)
        if session.stage != .done {
            // Preserve an explicit digest-level counterexample when the old pipeline overwrites inputs.
            let bytes = try Data(contentsOf: URL(fileURLWithPath: f.plan.tmpVarsPath))
            XCTAssertEqual(HvfWindowsInstallRecoveryFixture.digest(bytes), sealed.varsSHA256,
                "Same-session retry replaced original vars before the old journal rejected its digest")
            XCTAssertEqual(try Data(contentsOf: f.paths.journal), journalBytes,
                "The failure is against the original journal, not a new transaction")
            return
        }
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalVars), f.originalVars)
        let config = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: f.paths.config))
        XCTAssertEqual(config.installPending, false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.journal.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.pendingRequest.path))
    }
}
