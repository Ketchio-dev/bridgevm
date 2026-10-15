import Foundation
import XCTest
@testable import BridgeVMControl

final class VMCreationDestinationCleanupTests: XCTestCase {
    func testFirstRunCleanupPreservesAnyUnexpectedRegistrationEntry() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        for name in ["regular", "directory", "dangling"] {
            let destination = try FirstRunImportDestination(slug: name,
                libraryRoot: fixture.library, fileManager: .default)
            let marker = destination.root.appendingPathComponent("prepared-media")
            try Data("owned bytes".utf8).write(to: marker)
            let config = destination.root.appendingPathComponent("vm.json")
            switch name {
            case "regular": try Data("ambiguous registration".utf8).write(to: config)
            case "directory": try FileManager.default.createDirectory(at: config, withIntermediateDirectories: false)
            default: try FileManager.default.createSymbolicLink(atPath: config.path, withDestinationPath: "missing-target")
            }
            destination.removeIfOwned()
            XCTAssertTrue(NativeLibraryReader.entryMayExist(config))
            XCTAssertEqual(try Data(contentsOf: marker), Data("owned bytes".utf8))
        }
    }

    func testUnpublishedFirstRunPreparationStillCleansItsOwnedDirectory() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        let destination = try FirstRunImportDestination(slug: "unpublished",
            libraryRoot: fixture.library, fileManager: .default)
        try Data("owned bytes".utf8).write(to: destination.root.appendingPathComponent("prepared-media"))
        destination.removeIfOwned()
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.root.path))
    }

    func testExternalMediaSurvivesUnconfirmedRegistrationOutsideItsStorageRoot() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        guard case .created(let config)? = fixture.createExternal(persist: false) else {
            return XCTFail("prepare external fixture without publication")
        }
        let outcome = VMLibrary.createOutcome(config, rootURL: fixture.library) { data, url in
            VMRegistrationWriter.commit(data, to: url, replaceExisting: false, syncParent: { _ in
                throw CocoaError(.fileWriteUnknown)
            })
        }
        guard case .publishedButUnsynced = outcome else { return XCTFail("expected actual renamed registration") }
        XCTAssertFalse(outcome.isCommitted, "Boolean creation must keep reporting unconfirmed durability")
        VMCreationDestinationCleanup.removeIfUnpublished(fixture.storage.appendingPathComponent(config.slug),
            libraryRoot: fixture.library)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.library, id: config.slug), config)
        XCTAssertEqual(try fixture.ownedISO(config), fixture.guiBytes)
        XCTAssertEqual(try Data(contentsOf: fixture.guiISO), fixture.guiBytes)
    }
}
