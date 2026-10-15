import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionNativeTests: XCTestCase {
    func testActualCreationCanDeleteWithRealAbsentPairLease() throws {
        for external in [false, true] {
            let f = try HvfPendingDeletionFixture(external: external)
            let savedISO = try Data(contentsOf: f.iso)
            let request = f.bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName)
            let savedRequest = try Data(contentsOf: request)
            XCTAssertTrue(try withHelper(f) { LibraryDeletionProtection.delete(f.config, rootURL: f.library) })
            XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
            XCTAssertEqual(try Data(contentsOf: f.iso), savedISO)
            XCTAssertEqual(f.fm.fileExists(atPath: f.bundle.path), external)
            if external {
                XCTAssertEqual(try Data(contentsOf: request), savedRequest)
                XCTAssertEqual(try Data(contentsOf: f.bundle.appendingPathComponent("disks/installer.iso")), savedISO)
            }
        }
    }

    func testRealCompetingPendingLeaseRefusesDeletionUntilReleased() throws {
        let f = try HvfPendingDeletionFixture()
        let lease = try HvfMediaLeaseSession(executable: HvfMediaImportTestSupport.helper,
            disk: f.bundle.path + "/disks/hvf-target.raw", vars: f.bundle.path + "/metadata/hvf-vars.fd",
            mode: .pendingDelete)
        defer { lease.abort() }
        let saved = try Data(contentsOf: f.entry.appendingPathComponent("vm.json"))
        XCTAssertFalse(try withHelper(f) { LibraryDeletionProtection.delete(f.config, rootURL: f.library) })
        XCTAssertEqual(try Data(contentsOf: f.entry.appendingPathComponent("vm.json")), saved)
        try lease.finish()
        XCTAssertTrue(try withHelper(f) { LibraryDeletionProtection.delete(f.config, rootURL: f.library) })
    }

    func testInstalledAndRestoredPairsStillUseOrdinaryOwnership() throws {
        for restored in [false, true] {
            let f = try HvfPendingDeletionFixture()
            var installed = f.config
            installed.installPending = false
            try f.save(installed)
            let disk = f.bundle.appendingPathComponent("disks/hvf-target.raw")
            let vars = f.bundle.appendingPathComponent("metadata/hvf-vars.fd")
            try Data("disk".utf8).write(to: disk)
            try Data("vars".utf8).write(to: vars)
            let helper = HvfMediaImportTestSupport.helper
            if restored {
                let snapshot = f.root.appendingPathComponent("saved.snapshot")
                _ = try HvfWindowsSnapshotCommand.invoke(helper,
                    ["create", disk.path, vars.path, snapshot.path, installed.slug, "8"])
                _ = try HvfWindowsSnapshotCommand.invoke(helper, ["restore", snapshot.path, disk.path, vars.path])
            }
            let lease = try HvfMediaLeaseSession(executable: helper, disk: disk.path, vars: vars.path)
            defer { lease.abort() }
            XCTAssertFalse(try withHelper(f) { LibraryDeletionProtection.delete(installed, rootURL: f.library) })
            XCTAssertEqual(try Data(contentsOf: disk), Data("disk".utf8))
            try lease.finish()
            XCTAssertTrue(try withHelper(f) { LibraryDeletionProtection.delete(installed, rootURL: f.library) })
        }
    }

    private func withHelper<T>(_ f: HvfPendingDeletionFixture, action: () throws -> T) throws -> T {
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
        defer {
            if let prior { setenv("BRIDGEVM_REPO_ROOT", prior, 1) } else { unsetenv("BRIDGEVM_REPO_ROOT") }
        }
        return try action()
    }
}
