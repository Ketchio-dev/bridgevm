import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryMoveSelectionTests: XCTestCase {
    func testSuccessfulMovePreservesAnotherVMSelectedAfterAdmission() async throws {
        let f = try LibraryMoveSelectionFixture()
        try f.admit()
        // The active LibraryWorkspaceList row writes the same selectedID property.
        f.library.selectedID = f.other.slug
        XCTAssertTrue(f.library.selectedModel === f.otherModel)
        try await f.finish()
        try f.assertMoved()
        XCTAssertEqual(f.library.selectedID, f.other.slug)
        XCTAssertTrue(f.library.selectedModel === f.otherModel)
    }

    func testSuccessfulMovePreservesNewImportSelectedAfterAdmission() async throws {
        let f = try LibraryMoveSelectionFixture()
        try f.admit()
        f.library.selectedID = LibraryModel.firstRunImportSelectionID
        try await f.finish()
        try f.assertMoved()
        XCTAssertEqual(f.library.selectedID, LibraryModel.firstRunImportSelectionID)
        XCTAssertNil(f.library.selectedModel)
    }

    func testSuccessfulMoveRetainsSourceSelectionWithoutNewNavigation() async throws {
        let f = try LibraryMoveSelectionFixture()
        try f.admit()
        try await f.finish()
        try f.assertMoved()
        XCTAssertEqual(f.library.selectedID, f.source.slug)
        XCTAssertEqual(f.library.selectedModel?.config.bundlePath, f.assets.destination.path)
    }

    func testOccupiedDestinationFailurePreservesNewSelectionAndBothLocations() async throws {
        let f = try LibraryMoveSelectionFixture()
        let sentinel = try f.occupyDestination()
        try f.admit()
        f.library.selectedID = f.other.slug
        try await f.finish()
        XCTAssertNotNil(f.library.moveError)
        XCTAssertEqual(f.library.selectedID, f.other.slug)
        XCTAssertTrue(f.library.selectedModel === f.otherModel)
        try f.assets.assertReadable(f.source)
        XCTAssertEqual(try Data(contentsOf: sentinel), Data("owned occupied destination".utf8))
        XCTAssertFalse(VMRelocationJournal.isPending(f.source, rootURL: f.assets.libraryRoot))
    }
}
