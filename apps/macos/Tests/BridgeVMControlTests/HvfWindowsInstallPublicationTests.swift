import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallPublicationTests: XCTestCase {
    func testPublishedRegistrationAndPreparedInstallSurviveDirectorySyncFailure() throws {
        try assertSyncFailurePreservesInstall(separateStorage: true)
    }

    func testPublishedInstallSurvivesDirectorySyncFailureWithDefaultStorage() throws {
        try assertSyncFailurePreservesInstall(separateStorage: false)
    }

    private func assertSyncFailurePreservesInstall(separateStorage: Bool) throws {
        let fixture = try HVFCreationPublicationFixture(separateStorage: separateStorage)
        defer { fixture.remove() }
        let sourceISO = fixture.root.appendingPathComponent("selected.iso")
        // Only the creation factory runs; this tiny fixture cannot boot an installer.
        let isoBytes = Data("synthetic ISO fixture; not bootable".utf8)
        try isoBytes.write(to: sourceISO)
        let outcome = VMLibrary.createWindowsHVFInstall(
            name: "Published Install", isoPath: sourceISO.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            storageDir: fixture.storage, libraryRoot: fixture.library, persist: true,
            publish: fixture.publishWithSyncFailure)

        guard case .publishedButUnsynced(let config, let warning)? = outcome else {
            return XCTFail("expected retained install with publication warning")
        }
        try fixture.assertRetained(config, warning: warning)
        let savedRequest = try XCTUnwrap(fixture.requestBytes)
        let request = try JSONDecoder().decode(HvfWindowsInstallRequest.self, from: savedRequest)
        let bundle = fixture.bundle(config)
        let managedISO = bundle.appendingPathComponent("disks/installer.iso")
        XCTAssertEqual(config.installPending, true)
        XCTAssertEqual(request.isoPath, managedISO.path)
        XCTAssertNotEqual(request.isoPath, sourceISO.path)
        XCTAssertEqual(request.diskGiB, 64)
        XCTAssertEqual(request.isoSHA256, HvfWindowsInstallCacheIdentity.sha256File(sourceISO.path))
        XCTAssertEqual(try Data(contentsOf: managedISO), isoBytes)
        XCTAssertEqual(try Data(contentsOf: bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName)), savedRequest)
        XCTAssertEqual(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath), request)
        XCTAssertEqual(try Data(contentsOf: sourceISO), isoBytes)
    }

    func testNotPublishedRollsBackPreparedInstallInBothStorageLayouts() throws {
        for separateStorage in [true, false] {
            let fixture = try HVFCreationPublicationFixture(separateStorage: separateStorage)
            defer { fixture.remove() }
            let sourceISO = fixture.root.appendingPathComponent("selected.iso")
            let bytes = Data("original synthetic ISO".utf8)
            try bytes.write(to: sourceISO)
            let outcome = VMLibrary.createWindowsHVFInstall(
                name: "Refused Install", isoPath: sourceISO.path, diskGiB: 64,
                injectViogpu3d: false, driverPackageDir: nil,
                storageDir: fixture.storage, libraryRoot: fixture.library,
                publish: fixture.publishWithRenameRefusal)
            XCTAssertNil(outcome)
            try fixture.assertRolledBack()
            XCTAssertEqual(try Data(contentsOf: sourceISO), bytes)
        }
    }

    func testCommittedInstallReturnsCreatedAndRetainsRequest() throws {
        let fixture = try HVFCreationPublicationFixture(separateStorage: true)
        defer { fixture.remove() }
        let iso = fixture.root.appendingPathComponent("selected.iso")
        let bytes = Data("synthetic ISO".utf8)
        try bytes.write(to: iso)
        guard case .created(let config)? = VMLibrary.createWindowsHVFInstall(
            name: "Committed Install", isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            storageDir: fixture.storage, libraryRoot: fixture.library) else {
            return XCTFail("expected committed creation")
        }
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self,
            from: Data(contentsOf: fixture.registration(config))), config)
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: request.isoPath)), bytes)
        XCTAssertEqual(try Data(contentsOf: iso), bytes)
    }

    func testNonpersistentInstallReturnsCreatedWithoutCallingPublisher() throws {
        let fixture = try HVFCreationPublicationFixture(separateStorage: false)
        defer { fixture.remove() }
        let iso = fixture.root.appendingPathComponent("selected.iso")
        let bytes = Data("synthetic ISO".utf8)
        try bytes.write(to: iso)
        guard case .created(let config)? = VMLibrary.createWindowsHVFInstall(
            name: "Prepared Install", isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            libraryRoot: fixture.library, persist: false, publish: { _, _ in
                XCTFail("persist false must not publish")
                return .notPublished(CocoaError(.fileWriteUnknown))
            }) else { return XCTFail("expected nonpersistent creation") }
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.registration(config).path))
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: request.isoPath)), bytes)
        XCTAssertEqual(try Data(contentsOf: iso), bytes)
    }
}
