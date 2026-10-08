import XCTest
@testable import BridgeVMControl

final class HvfWindowsImportPublicationTests: XCTestCase {
    private let diskPrefix = Data("synthetic raw disk".utf8)
    private let varsPrefix = Data("synthetic UEFI vars".utf8)

    func testPublishedImportSurvivesDirectorySyncFailureWithSeparateStorage() throws {
        try assertSyncFailurePreservesImport(separateStorage: true)
    }

    func testPublishedImportSurvivesDirectorySyncFailureWithDefaultStorage() throws {
        try assertSyncFailurePreservesImport(separateStorage: false)
    }

    private func assertSyncFailurePreservesImport(separateStorage: Bool) throws {
        let fixture = try makeFixture(separateStorage: separateStorage)
        defer { fixture.remove() }
        let outcome = VMLibrary.createWindowsHVF(
            name: "Published Import", targetDiskPath: sourceDisk(fixture).path,
            varsPath: sourceVars(fixture).path, storageDir: fixture.storage,
            libraryRoot: fixture.library, snapshotHelper: HvfMediaImportTestSupport.helper,
            publish: fixture.publishWithSyncFailure)
        guard case .publishedButUnsynced(let config, let warning)? = outcome else {
            return XCTFail("expected retained import with publication warning")
        }
        try fixture.assertRetained(config, warning: warning)
        try assertImportedPair(config, fixture: fixture)
        try assertSources(fixture)
    }

    func testNotPublishedRollsBackPreparedImportInBothStorageLayouts() throws {
        for separateStorage in [true, false] {
            let fixture = try makeFixture(separateStorage: separateStorage)
            defer { fixture.remove() }
            let outcome = VMLibrary.createWindowsHVF(
                name: "Refused Import", targetDiskPath: sourceDisk(fixture).path,
                varsPath: sourceVars(fixture).path, storageDir: fixture.storage,
                libraryRoot: fixture.library, snapshotHelper: HvfMediaImportTestSupport.helper,
                publish: fixture.publishWithRenameRefusal)
            XCTAssertNil(outcome)
            try fixture.assertRolledBack()
            try assertSources(fixture)
        }
    }

    func testCommittedImportReturnsCreatedAndRetainsIndependentPair() throws {
        let fixture = try makeFixture(separateStorage: true)
        defer { fixture.remove() }
        guard case .created(let config)? = VMLibrary.createWindowsHVF(
            name: "Committed Import", targetDiskPath: sourceDisk(fixture).path,
            varsPath: sourceVars(fixture).path, storageDir: fixture.storage,
            libraryRoot: fixture.library, snapshotHelper: HvfMediaImportTestSupport.helper) else {
            return XCTFail("expected committed creation")
        }
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self,
            from: Data(contentsOf: fixture.registration(config))), config)
        try assertImportedPair(config, fixture: fixture)
        try assertSources(fixture)
    }

    func testNonpersistentImportReturnsCreatedWithoutCallingPublisher() throws {
        let fixture = try makeFixture(separateStorage: false)
        defer { fixture.remove() }
        guard case .created(let config)? = VMLibrary.createWindowsHVF(
            name: "Prepared Import", targetDiskPath: sourceDisk(fixture).path,
            varsPath: sourceVars(fixture).path, libraryRoot: fixture.library,
            persist: false, snapshotHelper: HvfMediaImportTestSupport.helper, publish: { _, _ in
                XCTFail("persist false must not publish")
                return .notPublished(CocoaError(.fileWriteUnknown))
            }) else { return XCTFail("expected nonpersistent creation") }
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.registration(config).path))
        try assertImportedPair(config, fixture: fixture)
        try assertSources(fixture)
    }

    private func makeFixture(separateStorage: Bool) throws -> HVFCreationPublicationFixture {
        let fixture = try HVFCreationPublicationFixture(separateStorage: separateStorage)
        do {
            try diskPrefix.write(to: sourceDisk(fixture))
            try varsPrefix.write(to: sourceVars(fixture))
            let handle = try FileHandle(forWritingTo: sourceVars(fixture))
            defer { try? handle.close() }
            try handle.truncate(atOffset: VMLibrary.windowsHVFVarsBytes)
            return fixture
        } catch { fixture.remove(); throw error }
    }

    private func sourceDisk(_ fixture: HVFCreationPublicationFixture) -> URL {
        fixture.root.appendingPathComponent("original.raw")
    }

    private func sourceVars(_ fixture: HVFCreationPublicationFixture) -> URL {
        fixture.root.appendingPathComponent("original-vars.fd")
    }

    private func assertImportedPair(_ config: VMConfig, fixture: HVFCreationPublicationFixture) throws {
        let bundle = fixture.bundle(config)
        let disk = URL(fileURLWithPath: try XCTUnwrap(config.diskPath))
        XCTAssertEqual(config.installPending, false)
        XCTAssertEqual(config.bundlePath, bundle.path)
        XCTAssertEqual(disk.path, bundle.appendingPathComponent("disks/hvf-target.raw").path)
        XCTAssertNotEqual(disk.path, sourceDisk(fixture).path)
        try assertFile(disk, prefix: diskPrefix, size: VMLibrary.minimumImportedWindowsHVFDiskGiB * 1024 * 1024 * 1024)
        try assertFile(bundle.appendingPathComponent("metadata/hvf-vars.fd"),
            prefix: varsPrefix, size: VMLibrary.windowsHVFVarsBytes)
        XCTAssertTrue(FileManager.default.fileExists(atPath: bundle.appendingPathComponent("metadata/hvf.ctl").path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: bundle.appendingPathComponent("metadata/hvf-grow-pending").path))
    }

    private func assertSources(_ fixture: HVFCreationPublicationFixture) throws {
        XCTAssertEqual(try Data(contentsOf: sourceDisk(fixture)), diskPrefix)
        try assertFile(sourceVars(fixture), prefix: varsPrefix, size: VMLibrary.windowsHVFVarsBytes)
    }

    private func assertFile(_ url: URL, prefix: Data, size: UInt64) throws {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        // Never read the entire 64 GiB sparse import into memory.
        XCTAssertEqual(try handle.read(upToCount: prefix.count), prefix)
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        XCTAssertEqual((attributes[.size] as? NSNumber)?.uint64Value, size)
    }
}
