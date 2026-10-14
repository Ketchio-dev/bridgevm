import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryCloneSelectionTests: XCTestCase {
    func testSuccessfulCloneKeepsAnotherVMSelectedDuringAcceptedWork() async throws {
        let f = try fixture()
        defer { f.clean() }
        f.library.selectedID = f.selected.slug
        f.admitClone(operation: publishCopy)
        // This is the same selection binding written by the enabled sidebar.
        f.library.selectedID = f.generic.slug
        let selectedModel = try XCTUnwrap(f.library.selectedModel)
        XCTAssertTrue(selectedModel === f.model)
        try await f.finishClone()
        XCTAssertEqual(f.library.selectedID, f.generic.slug)
        XCTAssertTrue(f.library.selectedModel === selectedModel)
        try assertCompleted(f, publishesCopy: true)
    }

    func testSuccessfulCloneKeepsNewImportSelectionDuringAcceptedWork() async throws {
        let f = try fixture()
        defer { f.clean() }
        f.library.selectedID = f.selected.slug
        f.admitClone(operation: publishCopy)
        f.library.selectedID = LibraryModel.firstRunImportSelectionID
        try await f.finishClone()
        XCTAssertEqual(f.library.selectedID, LibraryModel.firstRunImportSelectionID)
        XCTAssertNil(f.library.selectedModel)
        try assertCompleted(f, publishesCopy: true)
    }

    func testSuccessfulCloneStillSelectsCopyWithoutNewNavigation() async throws {
        let f = try fixture()
        defer { f.clean() }
        f.library.selectedID = f.selected.slug
        f.admitClone(operation: publishCopy)
        try await f.finishClone()
        XCTAssertEqual(f.library.selectedID, "copy")
        XCTAssertEqual(f.library.selectedModel?.config.slug, "copy")
        try assertCompleted(f, publishesCopy: true)
    }

    func testFailedCloneKeepsNewSelectionAndReportsFailure() async throws {
        let f = try fixture()
        defer { f.clean() }
        f.library.selectedID = f.selected.slug
        f.admitClone { _, _, _ in nil }
        f.library.selectedID = f.generic.slug
        try await f.finishClone()
        XCTAssertEqual(f.library.selectedID, f.generic.slug)
        XCTAssertTrue(f.library.selectedModel === f.model)
        try assertCompleted(f, publishesCopy: false)
    }

    private func fixture() throws -> ReservedCloneGenericFixture {
        try ReservedCloneGenericFixture(kind: "fast-vz", shared: false)
    }

    private var publishCopy: LibraryCloneOperation {
        { name, source, root in
            // Publish only a tiny invented registration; never copy media or touch keys.
            var result = source
            result.id = "copy"; result.name = name; result.displayName = name
            result.bundlePath = root.appendingPathComponent("copy/bundle").path
            result.diskPath = nil
            try? FileManager.default.createDirectory(atPath: result.bundlePath, withIntermediateDirectories: true)
            return VMLibrary.createOutcome(result, rootURL: root).isCommitted ? result : nil
        }
    }

    private func assertCompleted(_ f: ReservedCloneGenericFixture, publishesCopy: Bool) throws {
        XCTAssertTrue(f.library.cloningSlugs.isEmpty)
        XCTAssertEqual(f.library.cloneError == nil, publishesCopy)
        XCTAssertEqual(f.library.vms.contains { $0.slug == "copy" }, publishesCopy)
        XCTAssertEqual(VMLibrary.list(rootURL: f.assets.root).contains { $0.slug == "copy" }, publishesCopy)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.original)
        for (url, bytes) in f.registrations { XCTAssertEqual(try Data(contentsOf: url), bytes) }
        XCTAssertEqual(f.backend.calls[0], 0)
        XCTAssertEqual(f.backend.calls[2], 0)
        XCTAssertEqual(f.backend.calls[3], 0)
    }
}
