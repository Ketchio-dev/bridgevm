import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// Finalization seals exactly the staged media the installer wrote, keeps only
/// the copied bundle log after commit, and still resumes journals an earlier
/// build sealed over the fixed shared-/tmp names without widening what it admits.
final class HvfWindowsInstallStagingJournalTests: XCTestCase {
    typealias Fixture = HvfWindowsInstallStagingFixture

    func testCommittedInstallKeepsOnlyTheBundleLogAndRemovesStagedMedia() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.register()
        let log = Data("BVINSTALL synthetic evidence\n".utf8)
        let media = try f.writeInstallerOutputs(log: log)

        try HvfWindowsInstallFinalization.finalize(plan: f.plan, secureBootSeeder: Fixture.seeder)

        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.plan.bundleInstallLogPath)), log)
        for url in [media.target, media.vars, media.evidence] {
            XCTAssertFalse(FileManager.default.fileExists(atPath: url.path), "\(url.path) outlived the committed install")
        }
        let root = media.evidence.deletingLastPathComponent()
        if !Fixture.isSharedTemporary(root.path) {
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.path), "\(root.path) outlived the committed install")
        }
    }

    func testJournalSealsTheMediaPathsTheInstallerWasGiven() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.register()
        let media = try f.writeInstallerOutputs(log: Data())

        XCTAssertThrowsError(try f.finalize(stoppingAt: .prepared))

        let journal = try f.readJournal()
        XCTAssertEqual(journal.sourceDiskPath, media.target.path)
        XCTAssertEqual(journal.sourceVarsPath, media.vars.path)
        for path in [journal.sourceDiskPath, journal.sourceVarsPath] {
            XCTAssertFalse(Fixture.isSharedTemporary(path), "journal seals a shared-/tmp source: \(path)")
        }
    }

    // A crash at a boundary stops right after that phase's journal write.
    func testStagedSourceIsUnlinkedOnlyAfterItsCloneIsJournaled() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.register()
        let media = try f.writeInstallerOutputs(log: Data())

        XCTAssertThrowsError(try f.finalize(stoppingAt: .varsStaged))

        XCTAssertEqual(try f.readJournal().phase, .varsStaged)
        XCTAssertNil(Fixture.status(media.target), "the staged target outlived its journaled clone")
        XCTAssertNotNil(Fixture.status(media.vars), "vars were unlinked before the crash that followed their journal write")
        XCTAssertNotNil(Fixture.status(media.evidence))
    }

    func testJournalRefusesStagingReplacedByALink() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.register()
        let media = try f.writeInstallerOutputs(log: Data())
        XCTAssertThrowsError(try f.finalize(stoppingAt: .prepared))
        let root = media.evidence.deletingLastPathComponent()
        let moved = f.root.appendingPathComponent("moved-staging", isDirectory: true)
        try FileManager.default.moveItem(at: root, to: moved)
        try FileManager.default.createSymbolicLink(at: root, withDestinationURL: moved)

        let result = try f.reconcile()

        XCTAssertNotNil(result.issue)
        XCTAssertEqual(result.config.installPending, true)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.finalDisk.path))
        XCTAssertEqual(Set(try FileManager.default.contentsOfDirectory(atPath: moved.path)),
                       [media.target.lastPathComponent, media.vars.lastPathComponent, media.evidence.lastPathComponent])
        XCTAssertEqual(Fixture.status(root).map { $0.st_mode & S_IFMT }, S_IFLNK)
    }

    func testLegacySharedTemporaryJournalStillCommitsFromOwnedSources() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.makeLegacyJournal()

        let result = try f.reconcile()

        XCTAssertNil(result.issue)
        XCTAssertEqual(result.config.installPending, false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.journalURL.path))
        let disk = try FileManager.default.attributesOfItem(atPath: f.finalDisk.path)
        XCTAssertEqual((disk[.size] as? NSNumber)?.uint64Value, 4096)
        for url in [f.legacyTarget, f.legacyVars] {
            XCTAssertFalse(FileManager.default.fileExists(atPath: url.path), "\(url.path) was not consumed")
        }
    }

    func testLegacySharedTemporaryJournalRefusesALinkedSource() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.makeLegacyJournal()
        let moved = f.root.appendingPathComponent("elsewhere-target.raw")
        try FileManager.default.moveItem(at: f.legacyTarget, to: moved)
        try FileManager.default.createSymbolicLink(at: f.legacyTarget, withDestinationURL: moved)

        let result = try f.reconcile()

        XCTAssertNotNil(result.issue)
        XCTAssertEqual(result.config.installPending, true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.journalURL.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.finalDisk.path))
    }

    func testJournalNamingMediaOutsideThisBundlesStagingIsRefused() throws {
        let f = try Fixture()
        defer { f.remove() }
        try f.register()
        let media = try f.writeInstallerOutputs(log: Data())
        XCTAssertThrowsError(try f.finalize(stoppingAt: .prepared))
        let elsewhere = f.root.appendingPathComponent("other.vmbridge/metadata/hvf-install-staging", isDirectory: true)
        try FileManager.default.createDirectory(at: elsewhere, withIntermediateDirectories: true)
        let foreignDisk = elsewhere.appendingPathComponent("target.raw")
        try FileManager.default.copyItem(at: media.target, to: foreignDisk)
        var journal = try f.readJournal()
        journal.sourceDiskPath = foreignDisk.path
        try f.writeJournal(journal)

        let result = try f.reconcile()

        XCTAssertNotNil(result.issue)
        XCTAssertEqual(result.config.installPending, true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.journalURL.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.finalDisk.path))
    }
}
