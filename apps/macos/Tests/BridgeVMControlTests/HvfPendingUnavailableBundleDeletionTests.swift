import Darwin
import XCTest
@testable import BridgeVMControl

/// An install-pending external registration whose bundle is definitively absent
/// is removed without a lease; every other pending deletion keeps the guard.
final class HvfPendingUnavailableBundleDeletionTests: XCTestCase {
    func testAbsentExternalPendingBundleRemovesOnlyTheRegistration() throws {
        for detached in [true, false] {
            let f = try HvfPendingDeletionFixture(external: true)
            let external = f.root.appendingPathComponent("external")
            let kept: URL
            if detached { kept = try move(external, in: f) } else {
                try f.fm.removeItem(at: f.bundle)
                kept = external
            }
            let bytes = try tree(kept)
            XCTAssertEqual(VMLibrary.deletionImpact(for: f.config, rootURL: f.library), .registrationOnly)
            XCTAssertTrue(try f.delete(), "detached: \(detached)")
            XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
            XCTAssertFalse(exists(f.command), "no lease helper may run")
            XCTAssertEqual(try tree(kept), bytes)
            XCTAssertEqual(exists(external), !detached)
        }
    }

    @MainActor
    func testLibraryDeletionFlowRemovesDetachedPendingRegistration() async throws {
        let f = try HvfPendingDeletionFixture(external: true)
        let unplugged = try move(f.root.appendingPathComponent("external"), in: f)
        let bytes = try tree(unplugged)
        let jobs = HvfPendingUnavailableBundleJobs()
        let library = LibraryModel(rootURL: f.library, migrateLegacy: false,
                                   actionScheduler: { jobs.pending.append($0) }, startsModelsAutomatically: false)
        let listed = try XCTUnwrap(library.vms.first { $0.slug == f.config.slug })
        XCTAssertEqual(listed.installPending, true)
        library.requestDeletion(listed)
        XCTAssertEqual(library.pendingDeletion, listed)
        XCTAssertEqual(library.deletionImpact(for: listed), .registrationOnly)
        library.confirmDeletion(listed)
        let job = try XCTUnwrap(jobs.pending.first)
        await job()
        XCTAssertNil(library.deletionError)
        XCTAssertFalse(library.vms.contains { $0.slug == listed.slug })
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(try tree(unplugged), bytes)
    }

    func testDetachedPendingSiblingDoesNotBlockPendingDeletion() throws {
        let f = try HvfPendingDeletionFixture()
        let sibling = try createExternalSibling(in: f)
        let unplugged = try move(f.root.appendingPathComponent("sibling-external"), in: f)
        let bytes = try tree(unplugged)
        XCTAssertTrue(try f.delete())
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertTrue(try String(contentsOf: f.command).hasPrefix("lease-pending-delete\n"))
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: sibling.slug), sibling)
        XCTAssertEqual(try tree(unplugged), bytes)
    }

    func testPresentPendingBundleKeepsTheGuardedPendingLease() throws {
        let f = try HvfPendingDeletionFixture(external: true)
        XCTAssertFalse(try f.delete(refuse: true))
        XCTAssertTrue(try String(contentsOf: f.command).hasPrefix("lease-pending-delete\n"))
        XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path))
        let residue = try HvfPendingDeletionFixture(external: true)
        try residue.file("disks/hvf-target.raw")
        try residue.assertRefused()
        XCTAssertTrue(residue.fm.fileExists(atPath: residue.bundle.appendingPathComponent("disks/hvf-target.raw").path))
    }

    func testManagedPendingBundleNeverTakesRegistrationOnlyPath() throws {
        let removed = try HvfPendingDeletionFixture()
        try removed.fm.removeItem(at: removed.bundle)
        let stale = try HvfPendingDeletionFixture()
        var staleConfig = stale.config
        staleConfig.bundlePath = stale.root.appendingPathComponent("old-home/library/pending/bundle.vmbridge").path
        staleConfig.diskPath = staleConfig.bundlePath + "/disks/hvf-target.raw"
        try stale.save(staleConfig)
        let aliased = try HvfPendingDeletionFixture()
        let alias = aliased.root.appendingPathComponent("library-alias")
        try aliased.fm.createSymbolicLink(at: alias, withDestinationURL: aliased.library)
        var aliasedConfig = aliased.config
        aliasedConfig.bundlePath = alias.path + String(aliased.config.bundlePath.dropFirst(aliased.library.path.count))
        aliasedConfig.diskPath = aliasedConfig.bundlePath + "/disks/hvf-target.raw"
        try aliased.save(aliasedConfig)
        try aliased.fm.removeItem(at: aliased.bundle)
        XCTAssertEqual(VMLibrary.deletionImpact(for: removed.config, rootURL: removed.library), .managedBundleDeleted)
        XCTAssertEqual(VMLibrary.deletionImpact(for: staleConfig, rootURL: stale.library), .managedBundleDeleted)
        for (f, config) in [(removed, removed.config), (stale, staleConfig), (aliased, aliasedConfig)] {
            XCTAssertFalse(try f.delete(config))
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path))
            XCTAssertFalse(exists(f.command))
        }
        let iso = stale.entry.appendingPathComponent("bundle.vmbridge/disks/installer.iso")
        XCTAssertEqual(try Data(contentsOf: iso), try Data(contentsOf: stale.iso))
    }

    func testEntryResidueOrJournalRefusesRegistrationOnlyRemoval() throws {
        for name in ["notes.txt", HvfWindowsInstallRequest.fileName, HvfWindowsInstallRequest.doneFileName,
                     "metadata/hvf-install-finalization/journal.json", ".relocation-pending.json"] {
            let f = try HvfPendingDeletionFixture(external: true)
            _ = try move(f.root.appendingPathComponent("external"), in: f)
            let extra = f.entry.appendingPathComponent(name)
            try f.fm.createDirectory(at: extra.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data("residue".utf8).write(to: extra)
            XCTAssertFalse(try f.delete(), name)
            XCTAssertEqual(try Data(contentsOf: extra), Data("residue".utf8), name)
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path), name)
            XCTAssertFalse(exists(f.command), name)
        }
    }

    func testBundleLookupFailuresRefuse() throws {
        guard geteuid() != 0 else { throw XCTSkip("Permission test requires an unprivileged user") }
        for failure in ["EACCES", "ENOTDIR"] {
            let f = try HvfPendingDeletionFixture(external: true)
            let external = f.root.appendingPathComponent("external")
            if failure == "EACCES" {
                try f.fm.setAttributes([.posixPermissions: 0], ofItemAtPath: external.path)
            } else {
                _ = try move(external, in: f)
                try Data("not a directory".utf8).write(to: external)
            }
            defer { try? f.fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: external.path) }
            XCTAssertFalse(try f.delete(), failure)
            XCTAssertFalse(exists(f.command), failure)
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path), failure)
        }
    }

    func testSiblingWithoutAReadableRequestStillRefusesOtherwise() throws {
        guard geteuid() != 0 else { throw XCTSkip("Permission test requires an unprivileged user") }
        for failure in ["reference", "EACCES", "ENOTDIR", "relative"] {
            let f = try HvfPendingDeletionFixture()
            var sibling = try createExternalSibling(in: f)
            let external = f.root.appendingPathComponent("sibling-external")
            switch failure {
            case "EACCES": try f.fm.setAttributes([.posixPermissions: 0], ofItemAtPath: external.path)
            case "ENOTDIR":
                _ = try move(external, in: f)
                try Data("not a directory".utf8).write(to: external)
            default:
                _ = try move(external, in: f)
                if failure == "reference" { sibling.isoPath = f.bundle.appendingPathComponent("disks/installer.iso").path }
                if failure == "relative" { sibling.bundlePath = "sibling/bundle.vmbridge" }
                try JSONEncoder().encode(sibling).write(to: f.library.appendingPathComponent("sibling/vm.json"))
            }
            defer { try? f.fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: external.path) }
            try f.assertRefused()
            XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: sibling.slug), sibling, failure)
        }
    }

    func testStaleManagedPendingSiblingStillRefuses() throws {
        let f = try HvfPendingDeletionFixture()
        _ = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(name: "Sibling", isoPath: f.iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil, libraryRoot: f.library))
        var sibling = try NativeLibraryReader.readConfig(rootURL: f.library, id: "sibling")
        sibling.bundlePath = f.root.appendingPathComponent("old-home/sibling/bundle.vmbridge").path
        try JSONEncoder().encode(sibling).write(to: f.library.appendingPathComponent("sibling/vm.json"))
        XCTAssertEqual(VMLibrary.deletionImpact(for: sibling, rootURL: f.library), .managedBundleDeleted)
        try f.assertRefused()
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: "sibling"), sibling)
    }

    private func createExternalSibling(in f: HvfPendingDeletionFixture) throws -> VMConfig {
        _ = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(name: "Sibling", isoPath: f.iso.path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
            storageDir: f.root.appendingPathComponent("sibling-external"), libraryRoot: f.library))
        return try NativeLibraryReader.readConfig(rootURL: f.library, id: "sibling")
    }

    private func move(_ volume: URL, in f: HvfPendingDeletionFixture) throws -> URL {
        let unplugged = f.root.appendingPathComponent("unplugged-" + volume.lastPathComponent)
        try f.fm.moveItem(at: volume, to: unplugged)
        return unplugged
    }

    private func exists(_ url: URL) -> Bool {
        var info = stat()
        return lstat(url.path, &info) == 0
    }

    private func tree(_ root: URL) throws -> [String: Data] {
        var files: [String: Data] = [:]
        for item in try FileManager.default.subpathsOfDirectory(atPath: root.path) {
            files[item] = (try? Data(contentsOf: root.appendingPathComponent(item))) ?? Data()
        }
        XCTAssertFalse(files.isEmpty)
        return files
    }
}

@MainActor
private final class HvfPendingUnavailableBundleJobs {
    var pending: [LibraryActionJob] = []
}
