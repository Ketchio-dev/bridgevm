import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// The 64 GiB target, the UEFI vars and the install evidence belong in one
/// private directory of the VM bundle. Fixed names in the shared temporary
/// directory can be pre-created, linked or read by any other local user.
@MainActor
final class HvfWindowsInstallStagingTests: XCTestCase {
    typealias Fixture = HvfWindowsInstallStagingFixture

    func testInstallCommandStagesAllMediaInOneDirectoryInsideTheBundle() throws {
        let f = try Fixture()
        defer { f.remove() }
        let paths = try ["--target", "--vars", "--evidence-dir"].map { try f.argument($0) }
        for path in paths {
            XCTAssertFalse(Fixture.isSharedTemporary(path), "install media in the shared temporary directory: \(path)")
        }
        let roots = Set(paths.map { URL(fileURLWithPath: $0).deletingLastPathComponent().path })
        XCTAssertEqual(roots.count, 1, "target, vars and evidence must share one staging directory: \(roots)")
        let root = try XCTUnwrap(roots.first)
        XCTAssertTrue(root.hasPrefix(f.bundle.path + "/"), "staging directory \(root) is outside \(f.bundle.path)")
    }

    // The runner deletes and recreates --target/--vars only under /tmp/bridgevm-*.
    func testInstallCommandNeverAsksTheRunnerToRecreateMediaOutsideItsNamespace() throws {
        let f = try Fixture()
        defer { f.remove() }
        let command = f.plan.installCommand()
        for (flag, media) in [("--fresh-target-size", "--target"), ("--vars-template", "--vars")]
        where command.contains(flag) {
            let path = try f.argument(media)
            XCTAssertTrue(path.hasPrefix("/tmp/bridgevm-") || path.hasPrefix("/private/tmp/bridgevm-"),
                          "\(flag) makes run-hvf-windows-scripted-install.sh refuse \(path)")
        }
        XCTAssertFalse(command.contains("--cleanup-created-media"))
    }

    func testPreparedStagingIsOwnerOnlyAndHoldsTheBundledSeed() throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        try HvfWindowsInstallExecution().prepareMedia(f.plan)
        let media = try f.media()
        for directory in [media.evidence.deletingLastPathComponent(), media.evidence] {
            let status = try XCTUnwrap(Fixture.status(directory), "missing \(directory.path)")
            XCTAssertEqual(status.st_mode & S_IFMT, S_IFDIR, "\(directory.path) must be a real directory")
            XCTAssertEqual(status.st_mode & 0o7777, 0o700, "\(directory.path) must be private to its owner")
            XCTAssertEqual(status.st_uid, geteuid(), directory.path)
        }
        let vars = try XCTUnwrap(Fixture.status(media.vars), "missing \(media.vars.path)")
        XCTAssertEqual(vars.st_mode & S_IFMT, S_IFREG)
        XCTAssertEqual(vars.st_mode & 0o077, 0, "UEFI vars must not be group or world accessible")
        XCTAssertEqual(vars.st_uid, geteuid())
        XCTAssertEqual(try Data(contentsOf: media.vars), try HvfWindowsBootSeed.bundledSeed())
    }

    func testPreparedMediaCreatesTheInstallTargetExclusivelyAtTheRequestedSize() throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        try HvfWindowsInstallExecution().prepareMedia(f.plan)
        let target = try f.media().target
        let status = try XCTUnwrap(Fixture.status(target),
                                   "the app, not the runner's shared-/tmp recreate path, must create \(target.path)")
        XCTAssertEqual(status.st_mode & S_IFMT, S_IFREG)
        XCTAssertEqual(status.st_mode & 0o077, 0, "the install target must not be group or world accessible")
        XCTAssertEqual(status.st_uid, geteuid())
        XCTAssertEqual(status.st_nlink, 1)
        XCTAssertEqual(UInt64(status.st_size), f.plan.freshTargetSizeBytes)
    }

    func testPreparedMediaNeverWritesThroughPlantedLinks() throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        let foreign = f.root.appendingPathComponent("foreign", isDirectory: true)
        try FileManager.default.createDirectory(at: foreign, withIntermediateDirectories: true)
        let sentinel = foreign.appendingPathComponent("sentinel")
        try Data("foreign".utf8).write(to: sentinel)
        let media = try f.media()
        let root = media.evidence.deletingLastPathComponent()
        // A stale or hostile staging directory already links to data this install must not touch.
        if !FileManager.default.fileExists(atPath: root.path) {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        }
        try FileManager.default.createSymbolicLink(at: media.evidence, withDestinationURL: foreign)
        try FileManager.default.createSymbolicLink(at: media.vars, withDestinationURL: sentinel)

        let prepared = (try? HvfWindowsInstallExecution().prepareMedia(f.plan)) != nil

        XCTAssertEqual(try Data(contentsOf: sentinel), Data("foreign".utf8))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: foreign.path), ["sentinel"])
        guard prepared else { return }
        for url in [media.evidence, media.vars] {
            let status = try XCTUnwrap(Fixture.status(url), "missing \(url.path)")
            XCTAssertNotEqual(status.st_mode & S_IFMT, S_IFLNK, "\(url.path) is still the planted link")
        }
        let evidence = try XCTUnwrap(Fixture.status(media.evidence))
        XCTAssertEqual(evidence.st_mode & 0o7777, 0o700)
    }

    func testPreparedMediaRefusesALinkedStagingDirectory() throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        let root = try f.media().evidence.deletingLastPathComponent()
        guard !Fixture.isSharedTemporary(root.path) else {
            return XCTFail("install media are staged in the shared temporary directory \(root.path)")
        }
        let foreign = f.root.appendingPathComponent("foreign", isDirectory: true)
        try FileManager.default.createDirectory(at: foreign, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: root.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: root, withDestinationURL: foreign)

        XCTAssertThrowsError(try HvfWindowsInstallExecution().prepareMedia(f.plan))

        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: foreign.path), [])
        let link = try XCTUnwrap(Fixture.status(root))
        XCTAssertEqual(link.st_mode & S_IFMT, S_IFLNK, "a refused link must be left for inspection, not replaced")
    }

    // Another user who can write the bundle or its metadata could rename staging under the runner.
    func testPreparedMediaRefusesABundleOrMetadataOthersCanWrite() throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        let staging = try f.media().evidence.deletingLastPathComponent()
        for (directory, mode) in [(f.bundle, 0o775), (f.bundle.appendingPathComponent("metadata"), 0o757)] {
            try FileManager.default.setAttributes([.posixPermissions: mode], ofItemAtPath: directory.path)
            XCTAssertThrowsError(try HvfWindowsInstallExecution().prepareMedia(f.plan), directory.path)
            XCTAssertNil(Fixture.status(staging), "staging was created under \(directory.path)")
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: directory.path)
        }
        try HvfWindowsInstallExecution().prepareMedia(f.plan)
        XCTAssertNotNil(Fixture.status(staging))
    }

    func testFailedAttemptNeverRunsOrDeletesThroughALinkedStagingDirectory() async throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        let media = try f.media()
        let root = media.evidence.deletingLastPathComponent()
        guard !Fixture.isSharedTemporary(root.path) else {
            return XCTFail("install media are staged in the shared temporary directory \(root.path)")
        }
        let foreign = f.root.appendingPathComponent("foreign", isDirectory: true)
        try FileManager.default.createDirectory(at: foreign, withIntermediateDirectories: true)
        let names = [media.target.lastPathComponent, media.vars.lastPathComponent]
        for name in names { try Data(name.utf8).write(to: foreign.appendingPathComponent(name)) }
        try FileManager.default.createSymbolicLink(at: root, withDestinationURL: foreign)
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let process = HvfWindowsInstallStagingProcess()
        let session = HvfWindowsInstallSession(plan: f.plan, validate: { _ in nil }, schedule: queue.enqueue,
            execution: HvfWindowsInstallExecution(process: process, verifySource: { _ in true }))

        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        if !queue.jobs.isEmpty { await queue.runNext() }

        guard case .failed = session.stage else { return XCTFail("stage=\(session.stage)") }
        XCTAssertEqual(process.runs, 0, "the installer must not start against a refused staging directory")
        for name in names {
            XCTAssertEqual(try Data(contentsOf: foreign.appendingPathComponent(name)), Data(name.utf8), name)
        }
    }
}

@MainActor
private final class HvfWindowsInstallStagingProcess: HvfWindowsInstallProcessRunning {
    private(set) var runs = 0
    func reset() {}
    func cancel() {}
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
             progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        runs += 1
        return false
    }
}
