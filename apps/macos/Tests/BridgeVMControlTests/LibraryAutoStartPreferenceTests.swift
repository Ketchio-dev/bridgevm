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
    func testRunningHVFMenuAndSheetBothRefusePreferenceMutation() throws {
        let f = try HvfRuntimeSessionStoreFixture(); defer { f.clean() }
        let saved = f.config(); f.save(saved)
        let library = f.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertTrue(library.canSetStartWithBridgeVM(for: saved))
        session.connectionState = .booting
        XCTAssertFalse(session.libraryPreferencesEditable)
        XCTAssertFalse(library.canSetStartWithBridgeVM(for: saved), "Menu must be visibly disabled while busy")
        library.setStartWithBridgeVM(true, for: saved)
        XCTAssertNil(library.latestConfiguration(for: saved).startWithBridgeVM)
        XCTAssertNotNil(session.saveLibraryPreference("자동 시작 설정", publish: { _, _ in
            XCTFail("Running sheet must not publish"); return .committed
        }) { $0.startWithBridgeVM = true })
        session.connectionState = .stopped
    }
    func testNonHVFPreferenceDoesNotSaveWhileStartingOrRunning() throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        let saved = try XCTUnwrap(f.library.vms.first)
        XCTAssertTrue(VMLibrary.save(saved, rootURL: f.root))
        let model = f.library.model(for: saved)
        model.setLifecycleBusy(true)
        f.library.setStartWithBridgeVM(true, for: saved)
        XCTAssertNil(f.library.latestConfiguration(for: saved).startWithBridgeVM)
        model.setLifecycleBusy(false)
        model.running = true
        f.library.setStartWithBridgeVM(true, for: saved)
        XCTAssertNil(f.library.latestConfiguration(for: saved).startWithBridgeVM)
        model.running = false
        f.library.setStartWithBridgeVM(true, for: saved)
        XCTAssertEqual(f.library.latestConfiguration(for: saved).startWithBridgeVM, true)
    }
}
