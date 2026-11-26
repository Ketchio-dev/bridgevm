import Combine
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryInstallRowStatusTests: XCTestCase {
    func testAnExistingRowFollowsTheInstallWithoutRefreshingForWorkerOutput() throws {
        let fixture = HvfWindowsInstallStoreFixture()
        defer { fixture.clean() }
        let config = fixture.config()
        try fixture.save(config)
        let library = fixture.library()
        let row = LibraryWorkspaceRow(library: library, config: config, model: library.model(for: config), selected: true)
        XCTAssertEqual(row.status.text, "Installation required")
        XCTAssertNil(library.windowsInstallSessions.record(for: config.slug))

        let session = library.windowsInstallSession(for: config)
        var changes = 0
        let observer = library.objectWillChange.sink { changes += 1 }
        defer { observer.cancel() }
        session.transition(to: .preparingSource)
        XCTAssertEqual(row.status.text, "Preparing installation media")
        XCTAssertGreaterThan(changes, 0)
        let beforeOutput = changes
        session.appendLog("installer output")
        XCTAssertEqual(changes, beforeOutput)
        session.transition(to: .installing)
        XCTAssertEqual(row.status.text, "Installing Windows")
        session.finish(.failed("detailed diagnostic"))
        XCTAssertEqual(row.status.text, "Installation failed")
    }
}
