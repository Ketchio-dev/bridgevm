import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallCommittedCleanupTests: XCTestCase {
    func testPartlyRemovedCommittedTransactionUsesFinalArtifactsOnNextReconciliation() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .configStaged)
        defer { f.clean() }
        try await f.attempt()
        let first = HvfWindowsInstallFinalization.reconcile(config: f.config, libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder, removeTransaction: { _ in
                try Self.removeDisposableStaging(f.paths)
                throw CocoaError(.fileWriteUnknown)
            })
        XCTAssertEqual(first.config.installPending, false)
        XCTAssertNotNil(first.issue)
        XCTAssertTrue(HvfLibraryLaunchContext(config: first.config, rootURL: f.plan.libraryRoot)
            .readinessIssues.contains { $0.code == "install-cleanup-pending" })
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.paths.journal.path))
        guard case .pending = try HvfWindowsInstallRecovery.inspect(plan: f.plan) else {
            return XCTFail("committed cleanup residue must not authorize fresh installation")
        }
        let second = HvfWindowsInstallFinalization.reconcile(config: first.config, libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: { _, _ in XCTFail("must not seed again"); return Data() })
        XCTAssertNil(second.issue)
        XCTAssertEqual(second.config.installPending, false)
        XCTAssertFalse(HvfLibraryLaunchContext(config: second.config, rootURL: f.plan.libraryRoot)
            .readinessIssues.contains { $0.code == "install-cleanup-pending" })
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.pendingRequest.path))
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
        XCTAssertEqual(try Data(contentsOf: f.paths.finalVars), f.originalVars)
        f.assertNoSecondPipeline()
    }

    func testRecoveryOfCommittedResidueDoesNotRequireDiscardedStagedRequest() throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .committed)
        defer { f.clean() }
        try f.createInterruptedTransaction()
        try Self.removeDisposableStaging(f.paths)
        guard case .pending(let ticket) = try HvfWindowsInstallRecovery.inspect(plan: f.plan) else {
            return XCTFail("expected preserved committed transaction")
        }
        let config = try HvfWindowsInstallRecovery.recover(plan: f.plan, ticket: ticket,
            secureBootSeeder: { _, _ in XCTFail("must not seed again"); return Data() })
        XCTAssertEqual(config.installPending, false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
    }

    func testCorruptCommittedFinalDiskIsNotAcceptedAsCleanupOnlyFailure() throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .committed)
        defer { f.clean() }
        try f.createInterruptedTransaction()
        try Self.removeDisposableStaging(f.paths)
        try Data(repeating: 0x99, count: f.originalDisk.count).write(to: f.paths.finalDisk)
        let result = HvfWindowsInstallFinalization.reconcile(
            config: try HvfWindowsInstallFinalization.loadConfig(f.paths.config), libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder, removeTransaction: { _ in
                XCTFail("corrupt publication must not be cleaned as success")
            })
        XCTAssertEqual(result.config.installPending, true)
        XCTAssertTrue(result.issue?.contains("실행을 차단") == true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.paths.journal.path))
        XCTAssertNotEqual(try? HvfWindowsInstallRecovery.inspect(plan: f.plan), .fresh)
    }

    private static func removeDisposableStaging(_ p: HvfWindowsInstallFinalizationPaths) throws {
        for url in [p.stagedDisk, p.stagedVars, p.stagedProvisionedVars, p.stagedReceipt, p.stagedRequest, p.stagedConfig] {
            try FileManager.default.removeItem(at: url)
        }
    }
}
