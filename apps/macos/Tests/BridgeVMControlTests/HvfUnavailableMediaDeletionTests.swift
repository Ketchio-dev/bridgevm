import XCTest
@testable import BridgeVMControl

/// Installed external registrations whose bundle or both final media are
/// definitively absent are removed without the media lease. Every other HVF
/// deletion keeps the ordinary lease; lookup errors other than ENOENT refuse.
final class HvfUnavailableMediaDeletionTests: XCTestCase {
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
        library.requestDeletion(listed)
        XCTAssertEqual(library.pendingDeletion, listed)
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
        let managed = try HvfPendingDeletionFixture()
        let managedConfig = try install(managed, media: false)
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
        for (f, config) in [(managed, managedConfig), (present, presentConfig),
                            (partial, partialConfig), (aliased, aliasedConfig)] {
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
