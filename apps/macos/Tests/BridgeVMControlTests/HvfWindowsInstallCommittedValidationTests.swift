import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallCommittedValidationTests: XCTestCase {
    func testCommittedFinalValidationRefusesCorruptVarsReceiptAndDoneRequest() throws {
        for member in ["vars", "receipt", "request"] {
            let f = try HvfWindowsInstallRecoveryFixture(boundary: .committed)
            defer { f.clean() }
            try f.createInterruptedTransaction()
            switch member {
            case "vars": try Data(repeating: 0x9a, count: f.originalVars.count).write(to: f.paths.finalVars)
            case "receipt": try Data("not-json".utf8).write(to: f.paths.finalReceipt)
            default:
                var request = try Data(contentsOf: f.paths.doneRequest)
                request.append(0x20) // Decodable JSON, but not the sealed request bytes.
                try request.write(to: f.paths.doneRequest)
            }
            let result = HvfWindowsInstallFinalization.reconcile(
                config: try HvfWindowsInstallFinalization.loadConfig(f.paths.config), libraryRoot: f.plan.libraryRoot,
                secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder,
                removeTransaction: { _ in XCTFail("must not clean corrupt final member: \(member)") })
            XCTAssertEqual(result.config.installPending, true)
            XCTAssertEqual(try HvfWindowsInstallFinalization.loadConfig(f.paths.config).installPending, true)
            XCTAssertTrue(result.issue?.contains("실행을 차단") == true)
            XCTAssertTrue(FileManager.default.fileExists(atPath: f.paths.journal.path))
            XCTAssertNotEqual(try? HvfWindowsInstallRecovery.inspect(plan: f.plan), .fresh)
        }
    }

    func testSuccessfulCleanupUsesVerifiedConfigWithoutPostRemovalReloadOrWrite() throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .committed)
        defer { f.clean() }
        try f.createInterruptedTransaction()
        let saved = try HvfWindowsInstallFinalization.loadConfig(f.paths.config)
        let removedConfig = f.paths.config.appendingPathExtension("read-unavailable")
        let result = HvfWindowsInstallFinalization.reconcile(config: saved, libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder, removeTransaction: { transaction in
                try HvfWindowsInstallDurability.durableRemove(transaction)
                // Remove read availability after the validated commit and actual cleanup.
                // The reconciler must not respond by writing stale pending configuration.
                try FileManager.default.moveItem(at: f.paths.config, to: removedConfig)
            })
        XCTAssertEqual(result.config, saved)
        XCTAssertNil(result.issue)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.config.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertEqual(try HvfWindowsInstallFinalization.loadConfig(removedConfig), saved)
        try FileManager.default.moveItem(at: removedConfig, to: f.paths.config)
    }

    func testResidualTransactionWithoutJournalStillBlocksWritableLaunch() throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .committed)
        defer { f.clean() }
        try f.createInterruptedTransaction()
        let config = try HvfWindowsInstallFinalization.loadConfig(f.paths.config)
        try FileManager.default.removeItem(at: f.paths.journal)
        let context = HvfLibraryLaunchContext(config: config, rootURL: f.plan.libraryRoot)
        XCTAssertEqual(config.installPending, false)
        XCTAssertTrue(context.readinessIssues.contains { $0.code == "install-cleanup-pending" })
        let engine = try XCTUnwrap(HvfEngineConfig.libraryVM(config, rootURL: f.plan.libraryRoot))
        XCTAssertTrue(engine.readiness(repoRoot: f.root).launchBlockers.contains { $0.code == "install-cleanup-pending" })
        var diagnostics: [String] = []
        XCTAssertNotNil(HvfRuntimeLaunchReadiness.failure(config: engine, repoRoot: f.root) { diagnostics.append($0) })
        XCTAssertTrue(diagnostics.contains { $0.contains("install-cleanup-pending") })
        try FileManager.default.removeItem(at: f.paths.transaction)
        XCTAssertTrue(context.readinessIssues.isEmpty)
    }
}
