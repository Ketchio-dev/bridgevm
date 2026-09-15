import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeLibraryActionCacheIdentityTests: XCTestCase {
    func testCurrentRegistrationRefusesOldCachedBackendUntilIdleReload() throws {
        for entry in HvfRuntimeLibraryActionEntry.allCases {
            let fixture = HvfRuntimeLibraryActionFixture()
            defer { fixture.clean() }
            let original = fixture.config()
            fixture.save(original)
            let library = fixture.library()
            let oldModel = fixture.cacheFake(in: library, config: original)
            oldModel.busy = true
            var current = original
            current.bundlePath = fixture.root.appendingPathComponent("replacement/bundle").path
            current.memMiB = 8192
            fixture.save(current)
            library.reload()
            oldModel.busy = false
            XCTAssertEqual(library.vms, [current])
            XCTAssertEqual(oldModel.config, original)
            XCTAssertFalse(oldModel.hasAcceptedOperation)
            XCTAssertTrue(library.model(for: current) === oldModel)
            let before = try fixture.snapshot()
            let creations = fixture.modelCreations

            fixture.assertRefused([entry], config: current, library: library)

            XCTAssertEqual(fixture.queue.count, 0)
            XCTAssertEqual(fixture.modelCreations, creations)
            XCTAssertTrue(library.model(for: current) === oldModel)
            XCTAssertEqual(oldModel.config, original)
            XCTAssertEqual(try fixture.snapshot(), before)
            // A failed baseline may reserve a job; never run it or treat that reservation as completed.
            guard fixture.queue.count == 0 else { continue }
            library.reload()
            let replacement = fixture.cacheFake(in: library, config: current)
            XCTAssertFalse(replacement === oldModel)
            XCTAssertEqual(replacement.config, current)
            XCTAssertEqual(fixture.modelCreations, creations + 1)
            fixture.assertAccepted(.clone, config: current, library: library)
            XCTAssertEqual(fixture.queue.count, 1)
            XCTAssertEqual(try fixture.snapshot(), before)
        }
    }
}
