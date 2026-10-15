import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfCloneVarsTests: XCTestCase {
    func testRegularVarsCopiesIndependentFile() throws { try check(.regular, allowed: true) }
    func testVarsSymlinkRefusesBeforeCopyAndKeyCustody() throws { try check(.varsLink, allowed: false) }
    func testMetadataSymlinkRefusesBeforeCopyAndKeyCustody() throws { try check(.metadataLink, allowed: false) }

    private func check(_ layout: HvfCloneVarsFixture.Layout, allowed: Bool) throws {
        let f = try HvfCloneVarsFixture(layout)
        defer { f.clean() }
        let backend = GenericOwnedControlBackend()
        backend.plan = nil
        backend.setExternalRunning(false)
        let queue = HvfRuntimeLibraryActionQueue()
        defer { queue.discard() }
        let library = LibraryModel(rootURL: f.libraryRoot, migrateLegacy: false, actionScheduler: queue.enqueue,
            startsModelsAutomatically: false, modelFactory: { config in
                ControlModel(config: config, backend: backend, startsAutomatically: false)
            })
        _ = library.model(for: f.config)
        let registration = f.libraryRoot.appendingPathComponent("source/vm.json")
        let originalRegistration = try Data(contentsOf: registration)
        library.requestWindowsClone(f.config)
        XCTAssertEqual(library.pendingWindowsClone != nil, allowed)
        library.cloneWindowsHVF(f.config, name: "UI copy", operation: { _, _, _ in
            XCTFail("The captured UI job must never run in this test")
            return nil
        })
        XCTAssertEqual(queue.count, allowed ? 1 : 0)
        XCTAssertEqual(library.cloningSlugs.isEmpty, !allowed)
        if !allowed { XCTAssertNotNil(library.cloneError) }
        try f.checkCopy(allowed: allowed)
        XCTAssertEqual(try Data(contentsOf: registration), originalRegistration)
        XCTAssertEqual(backend.calls[0], 0)
        XCTAssertEqual(backend.calls[2], 0)
        XCTAssertEqual(backend.calls[3], 0)
    }
}
