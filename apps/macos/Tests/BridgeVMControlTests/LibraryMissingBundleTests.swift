import XCTest
@testable import BridgeVMControl

/// A VM whose files are on a disconnected drive. Move used to ask for a folder and then report
/// that BridgeVM “could not confirm the move” though nothing moved; Show in Finder opened nothing.
@MainActor
final class LibraryMissingBundleTests: XCTestCase {
    func testMoveAndShowInFinderAskForTheDriveWhenTheBundleIsGone() async throws {
        let f = try LibraryMoveSelectionFixture()
        let bundle = URL(fileURLWithPath: f.source.bundlePath)
        let unplugged = f.assets.root.appendingPathComponent("unplugged")
        try FileManager.default.moveItem(at: bundle, to: unplugged)
        let message = "BridgeVM can’t find this VM’s files. Connect the drive that holds them, then try again."

        // The menu asks before it opens the folder panel.
        XCTAssertFalse(f.library.admitBundleMove(f.source))
        XCTAssertEqual(f.library.moveError, message)
        var revealed: [URL] = []
        f.library.revealBundleInFinder(f.source) { revealed = $0 }
        XCTAssertEqual(f.library.operationError, message)
        XCTAssertTrue(revealed.isEmpty)

        // A drive pulled after the folder was chosen: the move fails with the same sentence.
        f.library.moveError = nil
        f.library.moveWindowsHVFBundle(f.source, to: f.assets.destinationParent)
        try await f.finish()
        XCTAssertEqual(f.library.moveError, message)
        XCTAssertFalse(VMRelocationJournal.isPending(f.source, rootURL: f.assets.libraryRoot))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.assets.destination.path))

        // With the drive back, both work as before.
        try FileManager.default.moveItem(at: unplugged, to: bundle)
        f.library.moveError = nil
        f.library.operationError = nil
        XCTAssertTrue(f.library.admitBundleMove(f.source))
        f.library.revealBundleInFinder(f.source) { revealed = $0 }
        XCTAssertNil(f.library.operationError)
        XCTAssertEqual(revealed, [bundle])
        try f.admit()
    }
}
