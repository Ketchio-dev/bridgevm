import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfCloneSourceAdmissionTests: XCTestCase {
    func testMigratedExternalSelectionRefusesSheetAndActionBeforeScheduling() throws {
        let fixture = try GenericOwnedControlFixture()
        let queue = HvfRuntimeLibraryActionQueue()
        defer { queue.discard(); fixture.clean() }
        let libraryRoot = fixture.root.appendingPathComponent("library")
        var source = try fixture.config("source", kind: "hvf-engine")
        let builtin = URL(fileURLWithPath: source.bundlePath).appendingPathComponent("disks/hvf-target.raw")
        let external = fixture.root.appendingPathComponent("selected.raw")
        try FileManager.default.createDirectory(at: builtin.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("bundled disk".utf8).write(to: builtin)
        try Data("selected disk".utf8).write(to: external)
        source.diskPath = external.path
        XCTAssertTrue(VMLibrary.migrateLegacyIfNeeded(rootURL: libraryRoot, legacy: source))
        let backend = GenericOwnedControlBackend()
        backend.plan = nil
        backend.setExternalRunning(false)
        let library = LibraryModel(rootURL: libraryRoot, migrateLegacy: false, actionScheduler: queue.enqueue,
            startsModelsAutomatically: false, modelFactory: { config in
                ControlModel(config: config, backend: backend, startsAutomatically: false)
            })
        XCTAssertTrue(library.model(for: source).backend === backend)
        let calls = backend.calls
        let registration = libraryRoot.appendingPathComponent("source/vm.json")
        let saved = try Data(contentsOf: registration)
        library.requestWindowsClone(source)
        XCTAssertNil(library.pendingWindowsClone)
        XCTAssertTrue(library.cloneError?.contains("가져온") == true)
        library.cloneError = nil
        library.cloneWindowsHVF(source, name: "Copy", operation: { _, _, _ in
            XCTFail("Refused source must not reach copy or key custody")
            return nil
        })
        XCTAssertEqual(queue.count, 0)
        XCTAssertTrue(library.cloningSlugs.isEmpty)
        XCTAssertTrue(library.cloneError?.contains("가져온") == true)
        XCTAssertEqual(backend.calls, calls)
        XCTAssertEqual(try Data(contentsOf: registration), saved)
        XCTAssertEqual(try Data(contentsOf: builtin), Data("bundled disk".utf8))
        XCTAssertEqual(try Data(contentsOf: external), Data("selected disk".utf8))
        XCTAssertFalse(FileManager.default.fileExists(atPath: libraryRoot.appendingPathComponent("copy").path))
    }
}
