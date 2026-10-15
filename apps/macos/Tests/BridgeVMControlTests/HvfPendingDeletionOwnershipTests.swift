import Darwin
import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionOwnershipTests: XCTestCase {
    func testStateIsRevalidatedAfterReady() throws {
        for path in ["disks/hvf-target.raw", "metadata/hvf-vars.fd", "metadata/hvf-install-staging",
                     "metadata/hvf-install-finalization", "metadata/hvf-install-done.json",
                     "metadata/secure-boot-provisioning.json", "disks/.bridgevm-pair-new", "metadata/vtpm"] {
            let f = try HvfPendingDeletionFixture()
            XCTAssertFalse(try f.delete(beforeReady: "mkdir '\(f.bundle.path)/\(path)'"))
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.path))
            XCTAssertTrue(f.fm.fileExists(atPath: f.command.path))
        }
    }

    func testSavedConfigAndRelocationAreRevalidatedAfterReady() throws {
        for changed in ["vm.json", ".relocation-pending.json", "bundle.vmbridge/metadata/hvf-install.json"] {
            let f = try HvfPendingDeletionFixture()
            XCTAssertFalse(try f.delete(beforeReady: "printf '{}' > '\(f.entry.path)/\(changed)'"))
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.path))
            XCTAssertTrue(f.fm.fileExists(atPath: f.command.path))
        }
    }

    func testHelperRefusalDoesNotDeleteOrFallBack() throws {
        let f = try HvfPendingDeletionFixture()
        XCTAssertFalse(try f.delete(refuse: true))
        XCTAssertTrue(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertTrue(try String(contentsOf: f.command).hasPrefix("lease-pending-delete\n"))
        try HvfPendingDeletionGuard.validate(f.config, rootURL: f.library)
    }

    func testInstalledConfigurationStillUsesOrdinaryLease() throws {
        let f = try HvfPendingDeletionFixture()
        var installed = f.config
        installed.installPending = false
        try f.save(installed)
        try f.file("disks/hvf-target.raw")
        try f.file("metadata/hvf-vars.fd")
        XCTAssertTrue(try f.delete(installed))
        XCTAssertTrue(try String(contentsOf: f.command).hasPrefix("lease\n"))
    }

    func testMissingOrMalformedSavedConfigIsRefusedWithoutRecovery() throws {
        for contents in [Data(), Data("{}".utf8), Data(repeating: 0, count: VMLibrary.maximumConfigBytes + 1)] {
            let f = try HvfPendingDeletionFixture()
            try contents.write(to: f.entry.appendingPathComponent("vm.json"))
            try f.assertRefused()
        }
        let f = try HvfPendingDeletionFixture()
        try f.fm.removeItem(at: f.entry.appendingPathComponent("vm.json"))
        try f.assertRefused()
    }

    func testMalformedOrOversizedInstallRequestIsRefused() throws {
        for contents in [Data(), Data("{}".utf8), Data(repeating: 0, count: VMLibrary.maximumConfigBytes + 1)] {
            let f = try HvfPendingDeletionFixture()
            try contents.write(to: f.bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName))
            try f.assertRefused()
        }
    }

    func testCanonicalDirectoriesCannotBeSymlinksOrMissing() throws {
        for name in ["disks", "metadata"] {
            for replacement in ["absent", "file", "symlink"] {
                let f = try HvfPendingDeletionFixture()
                let directory = f.bundle.appendingPathComponent(name)
                let moved = f.root.appendingPathComponent(name)
                try f.fm.moveItem(at: directory, to: moved)
                if replacement == "symlink" { try f.fm.createSymbolicLink(at: directory, withDestinationURL: moved) }
                if replacement == "file" { try Data().write(to: directory) }
                try f.assertRefused()
            }
        }
    }

    func testBundleAndLibraryAliasesAreRejected() throws {
        let f = try HvfPendingDeletionFixture()
        let moved = f.root.appendingPathComponent("moved-bundle")
        try f.fm.moveItem(at: f.bundle, to: moved)
        try f.fm.createSymbolicLink(at: f.bundle, withDestinationURL: moved)
        try f.assertRefused()
        try f.fm.removeItem(at: f.bundle)
        try f.fm.moveItem(at: moved, to: f.bundle)
        let alias = f.root.appendingPathComponent("library-alias")
        try f.fm.createSymbolicLink(at: alias, withDestinationURL: f.library)
        XCTAssertThrowsError(try HvfPendingDeletionGuard.validate(f.config, rootURL: alias))
    }

    func testLookupErrorsAreNotAbsence() throws {
        guard geteuid() != 0 else { throw XCTSkip("Permission test requires an unprivileged user") }
        let f = try HvfPendingDeletionFixture()
        let metadata = f.bundle.appendingPathComponent("metadata")
        try f.fm.setAttributes([.posixPermissions: 0o000], ofItemAtPath: metadata.path)
        defer { try? f.fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: metadata.path) }
        try f.assertRefused()
    }
}
