import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryMoveSelectionIdentityTests: XCTestCase {
    func testContextMenuMoveStillRevealsSourceIfAdmissionSelectionIsUnchanged() async throws {
        let f = try LibraryMoveSelectionFixture()
        f.library.selectedID = f.other.slug
        try f.admit()
        try await f.finish()
        try f.assertMoved()
        XCTAssertEqual(f.library.selectedID, f.source.slug)
        XCTAssertEqual(f.library.selectedModel?.config.bundlePath, f.assets.destination.path)
    }

    func testReturningToAdmissionSelectionStillRevealsMovedConfiguration() async throws {
        let f = try LibraryMoveSelectionFixture()
        try f.admit()
        f.library.selectedID = f.other.slug
        f.library.selectedID = f.source.slug
        try await f.finish()
        try f.assertMoved()
        XCTAssertEqual(f.library.selectedID, f.source.slug)
        XCTAssertEqual(f.library.selectedModel?.config.bundlePath, f.assets.destination.path)
    }
}
