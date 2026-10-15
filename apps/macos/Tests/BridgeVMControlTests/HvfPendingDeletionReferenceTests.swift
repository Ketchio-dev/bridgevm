import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionReferenceTests: XCTestCase {
    func testSavedSiblingSharingManagedBundleRefusesDeletionAndPreservesBytes() throws {
        let f = try HvfPendingDeletionFixture()
        var sibling = f.config
        sibling.id = "sibling"
        sibling.name = "Sibling"
        sibling.displayName = "Sibling"
        try save(sibling, in: f)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: sibling.slug), sibling)
        let source = f.bundle.appendingPathComponent("disks/installer.iso")
        let bytes = try Data(contentsOf: source)
        XCTAssertFalse(try f.delete(), "Deleting A must not remove B's saved bundle")
        XCTAssertEqual(try? Data(contentsOf: source), bytes)
        XCTAssertTrue(f.fm.fileExists(atPath: f.entry.path))
    }

    func testSiblingInstallRequestSourceUnderRemovedEntryRefusesDeletionAndPreservesBytes() throws {
        for bootMode in ["windows-hvf", nil, "iso-efi"] {
            let f = try HvfPendingDeletionFixture()
            var sibling = try createSibling(in: f)
            let source = f.bundle.appendingPathComponent("disks/installer.iso")
            let bytes = try Data(contentsOf: source)
            let requestURL = URL(fileURLWithPath: sibling.bundlePath)
                .appendingPathComponent(HvfWindowsInstallRequest.fileName)
            var request = try HvfWindowsInstallRequestSnapshot.load(requestURL).request
            request.isoPath = source.path
            XCTAssertTrue(request.save(bundlePath: sibling.bundlePath))
            XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: sibling.slug), sibling)
            sibling.bootMode = bootMode
            try save(sibling, in: f)
            XCTAssertFalse(try f.delete(), "A's final-pair lease does not protect B's installer source")
            XCTAssertEqual(try? Data(contentsOf: source), bytes)
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.path))
        }
    }

    func testSavedConfigPathFieldsCannotPointIntoRemovedEntry() throws {
        let mutations: [(inout VMConfig, String) -> Void] = [
            { $0.diskPath = $1 }, { $0.isoPath = $1 }, { $0.launchSpecPath = $1 },
            { $0.runnerPath = $1 }, { $0.handoffPath = $1 },
            { $0.sshKeyPath = $1 }, { $0.leasesPath = $1 }
        ]
        for mutation in mutations {
            let f = try HvfPendingDeletionFixture()
            var sibling = try createSibling(in: f)
            mutation(&sibling, f.entry.appendingPathComponent("source-input").path)
            try save(sibling, in: f)
            try f.assertRefused()
        }
    }

    func testInstallRequestSourceFieldsCannotPointIntoRemovedEntry() throws {
        let fields: [WritableKeyPath<HvfWindowsInstallRequest, String?>] = [
            \.driverPackageDir, \.guestPayloadDirectory, \.guestPayloadManifest, \.unattendedPath
        ]
        for field in fields {
            let f = try HvfPendingDeletionFixture()
            let sibling = try createSibling(in: f)
            var request = try HvfWindowsInstallRequestSnapshot.load(URL(fileURLWithPath: sibling.bundlePath)
                .appendingPathComponent(HvfWindowsInstallRequest.fileName)).request
            request[keyPath: field] = f.entry.appendingPathComponent("source-input").path
            XCTAssertTrue(request.save(bundlePath: sibling.bundlePath))
            try f.assertRefused()
        }
    }

    func testTwoRegistrationsSharingExternalBundleAllowRegistrationOnlyDeletion() throws {
        let f = try HvfPendingDeletionFixture(external: true)
        var sibling = f.config
        sibling.id = "sibling"
        try save(sibling, in: f)
        let request = f.bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName)
        let bytes = try Data(contentsOf: request)
        XCTAssertTrue(try f.delete())
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: sibling.slug), sibling)
        XCTAssertEqual(try Data(contentsOf: request), bytes)
        XCTAssertEqual(try Data(contentsOf: f.bundle.appendingPathComponent("disks/installer.iso")),
                       try Data(contentsOf: f.iso))
    }

    func testSimilarEntryPrefixIsNotAReferenceAndDerivedCacheIsNotARegistration() throws {
        let f = try HvfPendingDeletionFixture()
        var sibling = try createSibling(in: f)
        let unrelated = f.library.appendingPathComponent(f.config.slug + "-other/source")
        try f.fm.createDirectory(at: unrelated.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("external source".utf8).write(to: unrelated)
        sibling.isoPath = unrelated.path
        try save(sibling, in: f)
        try f.fm.createDirectory(at: f.library.appendingPathComponent("Derived/WindowsInstallSources"),
                                 withIntermediateDirectories: true)
        XCTAssertTrue(try f.delete())
        XCTAssertEqual(try Data(contentsOf: unrelated), Data("external source".utf8))
        XCTAssertTrue(f.fm.fileExists(atPath: sibling.bundlePath))
    }

    func testCanonicalAliasesAndDeletedSymlinkPathsAreReferences() throws {
        let f = try HvfPendingDeletionFixture()
        var sibling = try createSibling(in: f)
        let externalAlias = f.root.appendingPathComponent("alias")
        try f.fm.createSymbolicLink(at: externalAlias, withDestinationURL: f.entry)
        let deletedAlias = f.entry.appendingPathComponent("source-link")
        try f.fm.createSymbolicLink(at: deletedAlias, withDestinationURL: f.iso)
        for path in [externalAlias.appendingPathComponent("bundle.vmbridge/disks/installer.iso").path,
                     f.entry.path + "/../" + f.config.slug + "/source", deletedAlias.path, f.entry.path] {
            sibling.isoPath = path
            try save(sibling, in: f)
            try f.assertRefused()
        }
    }

    func testReferencesAreRecheckedAfterLeaseReady() throws {
        for requestOnly in [false, true] {
            let f = try HvfPendingDeletionFixture()
            var sibling = try createSibling(in: f)
            let replacement = f.root.appendingPathComponent("replacement.json")
            let destination: URL
            if requestOnly {
                destination = URL(fileURLWithPath: sibling.bundlePath)
                    .appendingPathComponent(HvfWindowsInstallRequest.fileName)
                var request = try HvfWindowsInstallRequestSnapshot.load(destination).request
                request.isoPath = f.bundle.appendingPathComponent("disks/installer.iso").path
                try JSONEncoder().encode(request).write(to: replacement)
            } else {
                destination = f.library.appendingPathComponent(sibling.slug).appendingPathComponent("vm.json")
                sibling.bundlePath = f.config.bundlePath
                try JSONEncoder().encode(sibling).write(to: replacement)
            }
            XCTAssertFalse(try f.delete(beforeReady: "/bin/cp '\(replacement.path)' '\(destination.path)'"))
            XCTAssertTrue(f.fm.fileExists(atPath: f.entry.path))
            XCTAssertEqual(try Data(contentsOf: f.bundle.appendingPathComponent("disks/installer.iso")),
                           try Data(contentsOf: f.iso))
        }
    }

    func testSuppliedRetainedRequestUsesSameReferencePredicateWithoutLoadingIt() throws {
        let f = try HvfPendingDeletionFixture()
        let sibling = try createSibling(in: f)
        let requestURL = URL(fileURLWithPath: sibling.bundlePath)
            .appendingPathComponent(HvfWindowsInstallRequest.fileName)
        var request = try HvfWindowsInstallRequestSnapshot.load(requestURL).request
        request.isoPath = f.bundle.appendingPathComponent("disks/installer.iso").path
        XCTAssertFalse(HvfPendingDeletionReferences.references(config: sibling, removedEntry: f.entry))
        XCTAssertTrue(HvfPendingDeletionReferences.references(config: sibling, request: request, removedEntry: f.entry))
        XCTAssertNotEqual(try HvfWindowsInstallRequestSnapshot.load(requestURL).request, request)
    }

    private func save(_ config: VMConfig, in f: HvfPendingDeletionFixture) throws {
        let entry = f.library.appendingPathComponent(config.slug)
        try f.fm.createDirectory(at: entry, withIntermediateDirectories: true)
        try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"), options: .atomic)
    }

    private func createSibling(in f: HvfPendingDeletionFixture) throws -> VMConfig {
        _ = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(name: "Sibling", isoPath: f.iso.path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil, libraryRoot: f.library))
        return try NativeLibraryReader.readConfig(rootURL: f.library, id: "sibling")
    }
}
