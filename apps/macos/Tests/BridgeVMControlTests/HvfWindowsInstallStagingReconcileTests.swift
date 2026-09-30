import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// A library scan finishes an interrupted finalization through reconcile. It must
/// leave the bundle a direct finalization leaves, including the install log, and
/// admit only staged sources this user owns.
final class HvfWindowsInstallStagingReconcileTests: XCTestCase {
    typealias Fixture = HvfWindowsInstallStagingFixture

    // Every boundary before configStaged, where finalization copies the log.
    func testReconcileAfterAnInterruptedFinalizationKeepsTheInstallLog() throws {
        let boundaries: [HvfWindowsInstallFinalizationBoundary] =
            [.prepared, .diskStaged, .varsStaged, .secureBootStaged, .requestStaged]
        for boundary in boundaries {
            try assertReconcileKeepsTheInstallLog(boundary.rawValue) { try $0.finalize(stoppingAt: boundary) }
        }
    }

    // A seeding failure leaves the journal at varsStaged for the next library scan.
    func testReconcileAfterAFailedSecureBootSeedKeepsTheInstallLog() throws {
        try assertReconcileKeepsTheInstallLog("seed failure") {
            try HvfWindowsInstallFinalization.finalize(
                plan: $0.plan, secureBootSeeder: { _, _ in throw Fixture.InjectedCrash() })
        }
    }

    // Staging is 0700, so only a hard link can put another user's file there;
    // sources an earlier build left in shared /tmp rely on the same check.
    func testStagedSourceAnotherUserOwnsIsRefused() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.register()
        let media = try f.writeInstallerOutputs(log: Data())
        XCTAssertThrowsError(try f.finalize(stoppingAt: .prepared))
        let foreign = "/private/etc/shells"
        try FileManager.default.removeItem(at: media.target)
        guard link(foreign, media.target.path) == 0 else {
            throw XCTSkip("cannot hard-link \(foreign) into the staging directory: errno \(errno)")
        }
        try XCTSkipIf(Fixture.status(media.target)?.st_uid == geteuid(), "\(foreign) is owned by the test user")
        // The journal seals the foreign bytes, so only ownership can refuse them.
        let identity = try HvfWindowsInstallFinalizationIdentity.seal(media.target)
        var journal = try f.readJournal()
        journal.diskBytes = identity.bytes
        journal.diskSHA256 = identity.sha256
        try f.writeJournal(journal)

        let result = try f.reconcile()

        XCTAssertTrue(result.issue?.contains(media.target.path) == true, result.issue ?? "no issue")
        XCTAssertEqual(result.config.installPending, true)
        XCTAssertEqual(try f.readJournal().phase, .prepared)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.finalDisk.path))
    }

    private func assertReconcileKeepsTheInstallLog(
        _ label: String, interrupt: (Fixture) throws -> Void
    ) throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.register()
        let log = Data("BVINSTALL \(label)\n".utf8)
        let staging = try f.writeInstallerOutputs(log: log).evidence.deletingLastPathComponent()
        XCTAssertThrowsError(try interrupt(f), label)

        let result = try f.reconcile()

        XCTAssertNil(result.issue, label)
        XCTAssertEqual(result.config.installPending, false, label)
        XCTAssertEqual(try? Data(contentsOf: f.bundle.appendingPathComponent("logs/install-run.log")), log,
                       "\(label): the install log was lost")
        XCTAssertNil(Fixture.status(staging), "\(label): staging outlived the commit")
    }
}
