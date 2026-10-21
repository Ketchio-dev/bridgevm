import XCTest
@testable import BridgeVMControl

/// Pins both sides of the retained lookup boundary: a terminal retained control stays
/// the cached control until its record is dismissed, and only dismissal yields a fresh
/// ordinary control for the current registration.
@MainActor
final class RetainedTerminalUndismissedTests: XCTestCase {
    func testTerminalRetainedControlStaysCachedUntilItsRecordIsDismissed() throws {
        let f = try RetainedTerminalRecoveryFixture()
        let (current, record) = try f.replaceWhileObservedRunning()
        f.accepted.applyRuntimeStatus(running: false, ip: nil)
        XCTAssertFalse(record.descriptor.isActive)
        XCTAssertTrue(f.library.model(for: current) === f.accepted, "Undismissed terminal record keeps its control")
        XCTAssertEqual(f.library.retainedControlRecords.map(\.id), [record.id])
        XCTAssertEqual(f.factory.models.count, 1)

        try f.retainedView(record).dismiss()
        let fresh = f.library.model(for: current)
        XCTAssertFalse(fresh === f.accepted, "Dismissal must release the retained control")
        XCTAssertFalse(fresh.isRetained)
        XCTAssertEqual(fresh.config, current)
        XCTAssertTrue(f.library.model(for: current) === fresh)
        XCTAssertEqual(f.factory.models.count, 2)
        XCTAssertTrue(f.accepted.isRetained)
    }
}
