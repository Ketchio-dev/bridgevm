import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryStartupCompatibilityTests: XCTestCase {
    func testOrdinaryOwnerFactoryAdoptsRecoveredLibraryOnce() async throws {
        let fixture = try LibraryStartupFixture()
        defer { fixture.clean() }
        try fixture.recovery.stageInterruption()
        let startup = NativeRuntimeLibraryStartup.make(rootURL: fixture.recovery.library,
            owner: fixture.owner, cache: fixture.cache,
            options: .init(e2eLibraryRoot: fixture.recovery.library, e2eUnattendedPath: nil))
        XCTAssertNil(fixture.cache.retainedValue)
        await startup.load()
        let model = try XCTUnwrap(startup.model)
        XCTAssertTrue(model === fixture.cache.retainedValue)
        XCTAssertEqual(model.vms.first?.installPending, false)
        XCTAssertEqual(model.selectedID, fixture.recovery.plan.slug)
        XCTAssertTrue(model.libraryIssues.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.recovery.paths.journal.path))
        await startup.load()
        XCTAssertTrue(startup.model === model)
        startup.close()
    }

    func testPreparedScanIsAdoptedWithoutReadingLibraryAgain() throws {
        let fixture = try LibraryStartupRecoveryFixture()
        defer { fixture.clean() }
        let scan = LibraryInitialScan.perform(rootURL: fixture.library, migrateLegacy: false)
        var changed = try XCTUnwrap(scan.configs.first)
        changed.displayName = "Changed after scan"
        XCTAssertTrue(VMLibrary.save(changed, rootURL: fixture.library))
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false, initialScan: scan,
            startsModelsAutomatically: false)
        XCTAssertEqual(model.vms, scan.configs)
        XCTAssertEqual(model.libraryIssues, scan.issues)
        model.reload()
        XCTAssertEqual(model.vms, [changed])
    }

    func testDefaultInitializationReloadAndAddKeepSynchronousReadback() throws {
        let fixture = try LibraryStartupRecoveryFixture()
        defer { fixture.clean() }
        try fixture.stageInterruption()
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false, startsModelsAutomatically: false)
        XCTAssertEqual(model.vms.first?.installPending, false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.paths.journal.path))
        var added = try XCTUnwrap(model.vms.first)
        added.id = "second"; added.name = "Second"; added.displayName = "Second"
        added.bundlePath = fixture.root.appendingPathComponent("second-bundle").path
        XCTAssertFalse(model.add(added))
        XCTAssertTrue(VMLibrary.save(added, rootURL: fixture.library))
        XCTAssertTrue(model.add(added))
        XCTAssertEqual(model.selectedID, added.slug)
        XCTAssertTrue(model.vms.contains(added))
        try FileManager.default.removeItem(at: fixture.library.appendingPathComponent(added.slug))
        model.reload()
        XCTAssertFalse(model.vms.contains(added))
        XCTAssertEqual(model.selectedID, fixture.plan.slug)
    }
}
