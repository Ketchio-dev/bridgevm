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
            oldModel.running = true // A running VM keeps its observed control across the reload.
            var current = original
            current.bundlePath = fixture.root.appendingPathComponent("replacement/bundle").path
            current.memMiB = 8192
            fixture.save(current)
            library.reload()
            oldModel.running = false
            XCTAssertEqual(library.vms, [current])
            XCTAssertEqual(oldModel.config, original)
            XCTAssertTrue(library.model(for: current) === oldModel)
            let before = try fixture.snapshot()
            let creations = fixture.modelCreations

            fixture.assertRefused([entry], config: current, library: library)
            XCTAssertEqual(entry.error(in: library),
                "The VM’s registration changed. When the original VM stops, press Command-K and choose Reload Library. Then try again.")

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

    /// A setting changed while Windows ran is saved when it stops, usually before the VM's control model
    /// polls it as stopped. Delete, Duplicate and Move then still refused with "the VM's registration
    /// changed" until the user reloaded the library by hand.
    func testStopTimeSettingSaveLeavesLibraryActionsAvailable() throws {
        let fixture = HvfRuntimeLibraryActionFixture()
        defer { fixture.clean() }
        let original = fixture.config()
        fixture.save(original)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: original))
        let observed = fixture.cacheFake(in: library, config: original)
        observed.running = true // its last poll saw Windows running
        session.connectionState = .booting
        library.setStartWithBridgeVM(true, for: original)
        session.connectionState = .stopped
        session.markStopped()
        fixture.processLookups = 0 // the save checked that no process uses the disk
        observed.running = false // its next poll
        let saved = library.latestConfiguration(for: original)
        XCTAssertEqual(saved.startWithBridgeVM, true)

        fixture.assertAccepted(.clone, config: saved, library: library)
        XCTAssertNil(library.cloneError)
        XCTAssertEqual(library.model(for: saved).config, saved)
    }
}
