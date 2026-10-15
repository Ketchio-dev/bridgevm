import Foundation
import XCTest
@testable import BridgeVMControl

/// A committed install whose transaction cleanup failed keeps a journal bound to
/// the current bundle path. Relocation must wait until that cleanup resolves.
@MainActor
final class HvfWindowsInstallCommittedCleanupRelocationTests: XCTestCase {
    private func installedWithCleanupResidue(_ f: HvfWindowsInstallRecoveryFixture) async throws -> VMConfig {
        try await f.attempt()
        let first = HvfWindowsInstallFinalization.reconcile(config: f.config, libraryRoot: f.plan.libraryRoot,
            secureBootSeeder: HvfWindowsInstallRecoveryFixture.syntheticSeeder,
            removeTransaction: { _ in throw CocoaError(.fileWriteUnknown) })
        XCTAssertEqual(first.config.installPending, false)
        XCTAssertNotNil(first.issue)
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.paths.journal.path))
        XCTAssertTrue(HvfLibraryLaunchContext(config: first.config, rootURL: f.plan.libraryRoot)
            .readinessIssues.contains { $0.code == "install-cleanup-pending" })
        return first.config
    }

    func testMoveDoesNotDemoteInstalledVMWithUnresolvedCleanup() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .configStaged)
        defer { f.clean() }
        let installed = try await installedWithCleanupResidue(f)
        let parent = f.root.appendingPathComponent("moved")

        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(installed, to: parent, rootURL: f.plan.libraryRoot),
                     "the preserved journal is bound to the current bundle path")
        let scan = VMLibrary.scan(rootURL: f.plan.libraryRoot)
        let current = try XCTUnwrap(scan.configs.first { $0.slug == installed.slug })
        XCTAssertEqual(current.installPending, false, "an installed VM must not be demoted")
        XCTAssertEqual(try HvfWindowsInstallFinalization.loadConfig(f.paths.config).installPending, false)
        XCTAssertFalse(scan.issues.contains { $0.message.contains("실행을 차단") }, "\(scan.issues)")
        XCTAssertEqual(current.bundlePath, installed.bundlePath)
        XCTAssertFalse(FileManager.default.fileExists(atPath: parent.appendingPathComponent("bundle").path))
        // Left in place, the next reconciliation completes the committed cleanup.
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.paths.transaction.path))
        XCTAssertEqual(try Data(contentsOf: f.paths.finalDisk), f.originalDisk)
    }

    func testLibraryMoveActionRefusesUnresolvedCleanupWithGuidance() async throws {
        let f = try HvfWindowsInstallRecoveryFixture(boundary: .configStaged)
        defer { f.clean() }
        let installed = try await installedWithCleanupResidue(f)
        let fm = FileManager.default, transaction = f.paths.transaction.path
        // The library's own scan cannot remove the residue either (permission failure).
        try fm.setAttributes([.posixPermissions: 0o555], ofItemAtPath: transaction)
        defer { try? fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: transaction) }
        let backend = HvfRuntimeLibraryActionBackend(), queue = HvfRuntimeLibraryActionQueue()
        let library = LibraryModel(rootURL: f.plan.libraryRoot, migrateLegacy: false,
            actionScheduler: queue.enqueue, startsModelsAutomatically: false,
            modelFactory: { ControlModel(config: $0, backend: backend, startsAutomatically: false) })
        defer { queue.discard() }
        try fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: transaction)
        let listed = try XCTUnwrap(library.vms.first { $0.slug == installed.slug })
        XCTAssertEqual(listed.installPending, false)
        XCTAssertTrue(fm.fileExists(atPath: f.paths.journal.path))
        XCTAssertTrue(library.model(for: listed).backend === backend)

        library.moveWindowsHVFBundle(listed, to: f.root.appendingPathComponent("moved"))

        XCTAssertEqual(queue.count, 0, "the move must not be scheduled")
        XCTAssertTrue(library.movingSlugs.isEmpty)
        XCTAssertEqual(backend.calls, .init(), "refused before a liveness probe")
        let message = try XCTUnwrap(library.moveError)
        XCTAssertTrue(message.contains("설치") && message.contains("transaction"), message)
        XCTAssertTrue(fm.fileExists(atPath: f.paths.journal.path))

        // A clone would copy the journal, which then fails validation at its new path.
        library.cloneWindowsHVF(listed, name: "Residue Clone")
        XCTAssertEqual(queue.count, 0, "the clone must not be scheduled")
        XCTAssertTrue(library.cloningSlugs.isEmpty)
        XCTAssertEqual(backend.calls, .init(), "refused before a liveness probe")
        let cloneMessage = try XCTUnwrap(library.cloneError)
        XCTAssertTrue(cloneMessage.contains("복제") && cloneMessage.contains("transaction"), cloneMessage)
    }
}
