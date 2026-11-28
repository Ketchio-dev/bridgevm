import XCTest
@testable import BridgeVMControl

/// Installed external registrations whose bundle or both final media are
/// definitively absent are removed without the media lease, and so is an installed
/// library VM without its disk and vars whose entry holds only BridgeVM's records.
/// Every other HVF deletion keeps the ordinary lease; lookup errors other than
/// ENOENT refuse.
final class HvfUnavailableMediaDeletionTests: XCTestCase {
    /// The lease opens the disk and vars, so a library VM whose files were removed (in Finder, say)
    /// failed every delete with “could not be removed from disk” and stayed in the library.
    @MainActor
    func testLibraryVMWithoutItsDiskAndVarsCanBeDeleted() async throws {
        let f = try HvfPendingDeletionFixture()
        let installed = try install(f, media: false)
        try f.file("logs/hvf/run.log")
        try f.file("metadata/vtpm/state.bin")
        let store = VTPMDeletionTestKeyStore()
        store.keys[installed.slug] = Data(repeating: 0x35, count: KeychainVTPMStateKeyStore.keyLength)
        let jobs = HvfUnavailableMediaDeletionJobs()
        let library = LibraryModel(rootURL: f.library, migrateLegacy: false,
            actionScheduler: { jobs.pending.append($0) },
            deletionOperation: { config, root, isRunning in
                LibraryDeletionKeyArchive.perform(config, rootURL: root, isRunning: isRunning, keyStore: store)
            }, startsModelsAutomatically: false)
        let listed = try XCTUnwrap(library.vms.first { $0.slug == installed.slug })
        library.confirmDeletion(listed)
        let restore = try useRealHelper(f)
        defer { restore() }
        let job = try XCTUnwrap(jobs.pending.first)
        await job()
        XCTAssertNil(library.deletionError)
        XCTAssertFalse(library.vms.contains { $0.slug == listed.slug })
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertNil(store.keys[installed.slug], "its vTPM key is archived as for any deleted VM")
    }

    /// Without the disk and vars, anything that may still hold the VM's data keeps the entry: a
    /// snapshot, a restored disk generation, install staging or a file BridgeVM never writes.
    func testLibraryVMWithoutItsDiskAndVarsKeepsAnythingOfValue() throws {
        for kept in ["metadata/snapshots/latest.snapshot/disk.raw", "disks/.bridgevm-pair-v2-0/current/disk.raw",
                     "metadata/hvf-install-finalization/disk.raw", "disks/copy.raw", "notes.txt"] {
            let f = try HvfPendingDeletionFixture()
            let installed = try install(f, media: false)
            try f.file(kept)
            let restore = try useRealHelper(f)
            defer { restore() }
            XCTAssertFalse(LibraryDeletionProtection.delete(installed, rootURL: f.library), kept)
            XCTAssertTrue(f.fm.fileExists(atPath: f.bundle.appendingPathComponent(kept).path), kept)
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path), kept)
        }
    }

    func testDetachedExternalVolumeRemovesOnlyTheRegistrationWithRealHelper() throws {
        let f = try HvfPendingDeletionFixture(external: true)
        let installed = try install(f, media: true)
        let unplugged = try detach(f)
        let kept = try tree(unplugged)
        XCTAssertEqual(VMLibrary.deletionImpact(for: installed, rootURL: f.library), .registrationOnly)
        let restore = try useRealHelper(f)
        defer { restore() }
        XCTAssertTrue(LibraryDeletionProtection.delete(installed, rootURL: f.library))
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(try tree(unplugged), kept)
        XCTAssertFalse(exists(f.root.appendingPathComponent("external")))
    }

    @MainActor
    func testLibraryDeletionFlowRemovesDetachedRegistration() async throws {
        let f = try HvfPendingDeletionFixture(external: true)
        _ = try install(f, media: true)
        let unplugged = try detach(f)
        let kept = try tree(unplugged)
        let jobs = HvfUnavailableMediaDeletionJobs()
        let library = LibraryModel(rootURL: f.library, migrateLegacy: false,
                                   actionScheduler: { jobs.pending.append($0) }, startsModelsAutomatically: false)
        let listed = try XCTUnwrap(library.vms.first { $0.slug == f.config.slug })
        XCTAssertEqual(library.requestDeletion(listed), listed)
        XCTAssertEqual(library.deletionImpact(for: listed), .registrationOnly)
        library.confirmDeletion(listed)
        let restore = try useRealHelper(f)
        defer { restore() }
        let job = try XCTUnwrap(jobs.pending.first)
        await job()
        XCTAssertNil(library.deletionError)
        XCTAssertFalse(library.vms.contains { $0.slug == listed.slug })
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(try tree(unplugged), kept)
    }

    func testUnavailableExternalMediaSkipsTheLease() throws {
        for detached in [true, false] {
            let f = try HvfPendingDeletionFixture(external: true)
            let installed = try install(f, media: false)
            let external = detached ? try detach(f) : f.root.appendingPathComponent("external")
            let kept = try tree(external)
            XCTAssertTrue(try f.delete(installed))
            XCTAssertFalse(exists(f.command), "the media lease helper must not run")
            XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
            XCTAssertEqual(try tree(external), kept)
        }
    }

    func testManagedPresentAndPartialMediaKeepTheOrdinaryLease() throws {
        // Managed media used to be absent here: that VM is now deleted without the lease
        // (testLibraryVMWithoutItsDiskAndVarsCanBeDeleted), so the managed case keeps its media.
        let managed = try HvfPendingDeletionFixture()
        let managedConfig = try install(managed, media: true)
        let present = try HvfPendingDeletionFixture(external: true)
        let presentConfig = try install(present, media: true)
        let partial = try HvfPendingDeletionFixture(external: true)
        let partialConfig = try install(partial, media: false)
        try Data("disk".utf8).write(to: partial.bundle.appendingPathComponent("disks/hvf-target.raw"))
        // An absent managed bundle spelled through a library alias still belongs to the entry.
        let aliased = try HvfPendingDeletionFixture()
        let alias = aliased.root.appendingPathComponent("library-alias")
        try aliased.fm.createSymbolicLink(at: alias, withDestinationURL: aliased.library)
        var aliasedConfig = try install(aliased, media: false)
        aliasedConfig.bundlePath = alias.path + String(aliased.config.bundlePath.dropFirst(aliased.library.path.count))
        aliasedConfig.diskPath = aliasedConfig.bundlePath + "/disks/hvf-target.raw"
        try aliased.save(aliasedConfig)
        try aliased.fm.removeItem(at: aliased.bundle)
        // It used to be refused here too. Its entry now holds only vm.json, so nothing else can go
        // with it: it is removed without the lease, which cannot open the missing files.
        XCTAssertTrue(try aliased.delete(aliasedConfig, refuse: true))
        XCTAssertFalse(exists(aliased.command), "the media lease helper must not run")
        XCTAssertFalse(aliased.fm.fileExists(atPath: aliased.entry.path))
        for (f, config) in [(managed, managedConfig), (present, presentConfig), (partial, partialConfig)] {
            XCTAssertFalse(try f.delete(config, refuse: true))
            XCTAssertEqual(try String(contentsOf: f.command).split(separator: "\n").first, "lease")
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path))
        }
        let lease = try HvfMediaLeaseSession(executable: HvfMediaImportTestSupport.helper,
            disk: present.bundle.path + "/disks/hvf-target.raw", vars: present.bundle.path + "/metadata/hvf-vars.fd")
        defer { lease.abort() }
        let restore = try useRealHelper(present)
        defer { restore() }
        XCTAssertFalse(LibraryDeletionProtection.delete(presentConfig, rootURL: present.library))
        try lease.finish()
        XCTAssertTrue(LibraryDeletionProtection.delete(presentConfig, rootURL: present.library))
        XCTAssertEqual(try Data(contentsOf: present.bundle.appendingPathComponent("disks/hvf-target.raw")),
                       Data("disk".utf8))
    }

    func testLookupFailuresRefuseWithoutTheLease() throws {
        for failure in ["EACCES", "ENOTDIR"] {
            let f = try HvfPendingDeletionFixture(external: true)
            let installed = try install(f, media: true)
            let external = f.root.appendingPathComponent("external")
            if failure == "EACCES" {
                try f.fm.setAttributes([.posixPermissions: 0], ofItemAtPath: external.path)
            } else {
                _ = try detach(f)
                try Data("not a directory".utf8).write(to: external)
            }
            defer { try? f.fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: external.path) }
            XCTAssertFalse(try f.delete(installed), failure)
            XCTAssertFalse(exists(f.command), failure)
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path), failure)
        }
    }

    /// A managed bundle whose saved absolute path went stale (renamed home,
    /// remounted volume) looks external but still lives inside the entry.
    func testManagedBundleBehindAStaleSavedPathIsNeverDeleted() throws {
        let f = try HvfPendingDeletionFixture()
        var installed = try install(f, media: true)
        let realDisk = f.bundle.appendingPathComponent("disks/hvf-target.raw")
        let stale = f.root.appendingPathComponent("old-home/library/\(installed.slug)/bundle.vmbridge").path
        installed.bundlePath = stale
        installed.diskPath = stale + "/disks/hvf-target.raw"
        try f.save(installed)
        XCTAssertEqual(VMLibrary.deletionImpact(for: installed, rootURL: f.library), .managedBundleDeleted)
        let restore = try useRealHelper(f)
        defer { restore() }
        XCTAssertFalse(LibraryDeletionProtection.delete(installed, rootURL: f.library))
        XCTAssertEqual(try Data(contentsOf: realDisk), Data("disk".utf8))
        XCTAssertTrue(f.fm.fileExists(atPath: f.entry.appendingPathComponent("vm.json").path))
    }

    private func install(_ f: HvfPendingDeletionFixture, media: Bool) throws -> VMConfig {
        var installed = f.config
        installed.installPending = false
        try f.save(installed)
        if media {
            try Data("disk".utf8).write(to: f.bundle.appendingPathComponent("disks/hvf-target.raw"))
            try Data("vars".utf8).write(to: f.bundle.appendingPathComponent("metadata/hvf-vars.fd"))
        }
        return installed
    }

    private func detach(_ f: HvfPendingDeletionFixture) throws -> URL {
        let unplugged = f.root.appendingPathComponent("unplugged")
        try f.fm.moveItem(at: f.root.appendingPathComponent("external"), to: unplugged)
        return unplugged
    }

    private func exists(_ url: URL) -> Bool {
        (try? FileManager.default.attributesOfItem(atPath: url.path)) != nil
    }

    private func tree(_ root: URL) throws -> [String: Data] {
        var files: [String: Data] = [:]
        let items = try FileManager.default.subpathsOfDirectory(atPath: root.path)
        for item in items {
            files[item] = (try? Data(contentsOf: root.appendingPathComponent(item))) ?? Data()
        }
        XCTAssertFalse(files.isEmpty)
        return files
    }

    private func useRealHelper(_ f: HvfPendingDeletionFixture) throws -> () -> Void {
        let repository = f.root.appendingPathComponent("native-helper-repo")
        let helper = repository.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        let wrapper = repository.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        if !f.fm.fileExists(atPath: helper.path) {
            for path in [helper, wrapper] {
                try f.fm.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
            }
            try f.fm.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
            try Data("#!/bin/sh\nexit 99\n".utf8).write(to: wrapper)
            try f.fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: wrapper.path)
        }
        let prior = ProcessInfo.processInfo.environment["BRIDGEVM_REPO_ROOT"]
        setenv("BRIDGEVM_REPO_ROOT", repository.path, 1)
        return { if let prior { setenv("BRIDGEVM_REPO_ROOT", prior, 1) } else { unsetenv("BRIDGEVM_REPO_ROOT") } }
    }
}

@MainActor
private final class HvfUnavailableMediaDeletionJobs {
    var pending: [LibraryActionJob] = []
}
