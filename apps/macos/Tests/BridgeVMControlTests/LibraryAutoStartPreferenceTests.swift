import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryAutoStartPreferenceTests: XCTestCase {
    func testContextMenuStoppedHVFSaveRebindsExistingSession() throws {
        let f = try HvfRuntimeSessionStoreFixture(); defer { f.clean() }
        let saved = f.config(); f.save(saved)
        let library = f.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        library.setStartWithBridgeVM(true, for: saved)
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.startWithBridgeVM, true)
        XCTAssertTrue(library.hvfRuntimeSessions.owns(session, for: latest),
                      "Context-menu save must rebind the stopped session like the sheet")
        XCTAssertEqual(session.config.libraryContext?.config.startWithBridgeVM, true)
        f.processLookups = 0 // A real preference save checks whether the disk is in use.
    }
    /// The setting only matters when BridgeVM opens, so it changes while Windows runs: the menu and the
    /// sheet show the new value at once, and vm.json (kept as admitted while running) gets it at the stop.
    /// Both refused it until Windows was shut down.
    func testRunningHVFMenuAndSheetTakeTheChangeAndSaveItAtStop() throws {
        let f = try HvfRuntimeSessionStoreFixture(); defer { f.clean() }
        let saved = f.config(); f.save(saved)
        let library = f.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        session.connectionState = .booting
        XCTAssertFalse(session.libraryPreferencesEditable)
        XCTAssertTrue(library.canSetStartWithBridgeVM(for: saved), "Menu stays enabled while running")
        library.setStartWithBridgeVM(true, for: saved)
        XCTAssertNil(library.latestConfiguration(for: saved).startWithBridgeVM, "vm.json waits for the stop")
        XCTAssertTrue(library.startsWithBridgeVM(saved), "the menu shows the change")
        XCTAssertTrue(session.startsWithBridgeVM, "and so does the sheet")
        session.connectionState = .stopped
        session.markStopped()
        XCTAssertEqual(library.latestConfiguration(for: saved).startWithBridgeVM, true)
        XCTAssertNil(HvfHostSettingLiveSave.pending[saved.slug])
        f.processLookups = 0
    }
    func testRetiredEngineVMNeitherSavesNorAutoStarts() throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        let saved = try XCTUnwrap(f.library.vms.first)
        XCTAssertEqual(saved.engineKind, .unsupported)
        XCTAssertTrue(VMLibrary.save(saved, rootURL: f.root))
        let registration = f.root.appendingPathComponent(saved.slug + "/vm.json")
        let bytes = try Data(contentsOf: registration)
        XCTAssertFalse(f.library.canSetStartWithBridgeVM(for: saved), "Menu must be visibly disabled")
        f.library.setStartWithBridgeVM(true, for: saved)
        XCTAssertEqual(try Data(contentsOf: registration), bytes)
        var enabled = saved
        enabled.startWithBridgeVM = true
        XCTAssertFalse(ControlAutoStart.isStartable(enabled, library: f.library))
    }
}
