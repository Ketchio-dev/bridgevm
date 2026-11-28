import XCTest
@testable import BridgeVMControl

/// After Windows shuts down (from its own Start menu, say) or a start fails, the VM window stays open
/// on "Windows has shut down." It offered only Close Window and Open Library; its Start Windows button
/// starts the VM with the saved settings, as the library's Start does, and keeps a refusal on screen.
@MainActor
final class HvfStoppedWindowStartTests: XCTestCase {
    func testTheStoppedWindowStartsWindowsAgainWithTheSavedSettings() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config("stopped-window"); fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        session.guiStartWorker = { _ in HvfGUIStartWorkerOutput(result: .refused("fixture: launch suppressed")) }

        session.openSettingsSheets.insert(ObjectIdentifier(fixture))
        XCTAssertEqual(session.startFromStoppedWindow(saved, in: library), "Close Settings before starting Windows.")
        XCTAssertNil(session.guiStartOperation)
        session.openSettingsSheets.removeAll()

        XCTAssertNil(session.startFromStoppedWindow(saved, in: library))
        let start = try XCTUnwrap(session.guiStartOperation, "the start was admitted")
        XCTAssertEqual(start.configuration.libraryContext?.config.slug, saved.slug)
        while session.hasPendingGUIStart { try await Task.sleep(nanoseconds: 5_000_000) }
    }
}
