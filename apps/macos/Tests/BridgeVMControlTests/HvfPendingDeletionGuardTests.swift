import Darwin
import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionGuardTests: XCTestCase {
    func testFreshCreationUsesPendingCommandAndRemovesOnlyManagedEntry() throws {
        let f = try HvfPendingDeletionFixture()
        try HvfPendingDeletionGuard.validate(f.config, rootURL: f.library)
        XCTAssertTrue(try f.delete())
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(try Data(contentsOf: f.iso), Data("synthetic installer bytes".utf8))
        XCTAssertEqual(try String(contentsOf: f.command).components(separatedBy: "\n"), [
            "lease-pending-delete", f.bundle.path + "/disks/hvf-target.raw",
            f.bundle.path + "/metadata/hvf-vars.fd", ""
        ])
    }

    func testExternalStorageOnlyRemovesRegistration() throws {
        let f = try HvfPendingDeletionFixture(external: true)
        let stagedISO = f.bundle.appendingPathComponent("disks/installer.iso")
        let request = f.bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName)
        let originalRequest = try Data(contentsOf: request)
        XCTAssertTrue(try f.delete())
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertTrue(f.fm.fileExists(atPath: f.bundle.path))
        XCTAssertEqual(try Data(contentsOf: stagedISO), try Data(contentsOf: f.iso))
        XCTAssertEqual(try Data(contentsOf: request), originalRequest)
    }

    func testSavedConfigMismatchIsRefused() throws {
        let mutations: [(inout VMConfig) -> Void] = [
            { $0.displayName = "Changed" }, { $0.installPending = false },
            { $0.diskPath = "/unrelated.raw" }, { $0.bundlePath += "-other" }
        ]
        for mutation in mutations {
            let f = try HvfPendingDeletionFixture()
            var saved = f.config
            mutation(&saved)
            try f.save(saved)
            try f.assertRefused()
        }
    }

    func testOnlyExpectedNativeWindowsBundleOwnedMediaAreAdmitted() throws {
        let mutations: [(inout VMConfig) -> Void] = [
            { $0.bootMode = "iso-efi" }, { $0.backendKind = "qemu-compat" },
            { $0.diskPath = nil }, { $0.diskPath = "/unrelated.raw" },
            { $0.diskPath = $0.bundlePath + "/disks/other.raw" },
            { $0.diskPath = $0.bundlePath + "/disks/../disks/hvf-target.raw" }
        ]
        for mutation in mutations {
            let f = try HvfPendingDeletionFixture()
            var selected = f.config
            mutation(&selected)
            try f.save(selected)
            XCTAssertThrowsError(try HvfPendingDeletionGuard.validate(selected, rootURL: f.library))
            // Non-HVF deletion deliberately has different registration semantics.
            if selected.engineKind == .hvfEngine {
                XCTAssertFalse(try f.delete(selected))
                XCTAssertFalse(f.fm.fileExists(atPath: f.command.path))
            }
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.path))
        }
    }

    func testFinalPairPartialOrCompleteIsNotPristine() throws {
        for paths in [["disks/hvf-target.raw"], ["metadata/hvf-vars.fd"],
                      ["disks/hvf-target.raw", "metadata/hvf-vars.fd"]] {
            let f = try HvfPendingDeletionFixture()
            for path in paths { try f.file(path) }
            try f.assertRefused()
        }
    }

    func testEveryInstallResidueIncludingEmptyStagingAndManagedRootsIsRefused() throws {
        for path in ["metadata/hvf-install-staging", "metadata/hvf-install-staging/evidence",
                     "metadata/hvf-install-finalization", "disks/.bridgevm-pair-v2-test",
                     "disks/.bridgevm-pair-legacy", "disks/.bridgevm-init-unpublished",
                     "metadata/.bridgevm-pair-unbound"] {
            let f = try HvfPendingDeletionFixture()
            try f.directory(path)
            try f.assertRefused()
        }
        for path in ["metadata/hvf-install-finalization/journal.json", "metadata/hvf-install-done.json",
                     "metadata/secure-boot-provisioning.json", "metadata/hvf-install-staging/target.raw"] {
            let f = try HvfPendingDeletionFixture()
            try f.file(path)
            try f.assertRefused()
        }
    }

    func testDanglingResidueAndMediaLinksAreNotAbsence() throws {
        for path in ["disks/hvf-target.raw", "metadata/hvf-vars.fd", "metadata/hvf-install-staging",
                     "metadata/hvf-install-finalization", "metadata/hvf-install-done.json",
                     "metadata/secure-boot-provisioning.json", "disks/.bridgevm-pair-orphan"] {
            let f = try HvfPendingDeletionFixture()
            try f.danglingLink(path)
            try f.assertRefused()
        }
    }

    func testRequestMustBePresentAndRegular() throws {
        for replacement in ["absent", "directory", "link", "fifo"] {
            let f = try HvfPendingDeletionFixture()
            let request = f.bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName)
            try f.fm.removeItem(at: request)
            switch replacement {
            case "directory": try f.directory(HvfWindowsInstallRequest.fileName)
            case "link": try f.danglingLink(HvfWindowsInstallRequest.fileName)
            case "fifo": XCTAssertEqual(mkfifo(request.path, 0o600), 0)
            default: break
            }
            try f.assertRefused()
        }
    }

    func testRelocationMarkerIncludingDanglingLinkIsRefused() throws {
        for link in [false, true] {
            let f = try HvfPendingDeletionFixture()
            let marker = VMRelocationJournal.url(f.config, rootURL: f.library)
            if link { try f.fm.createSymbolicLink(at: marker, withDestinationURL: f.root.appendingPathComponent("missing")) }
            else { try Data("pending".utf8).write(to: marker) }
            try f.assertRefused()
        }
    }
}
