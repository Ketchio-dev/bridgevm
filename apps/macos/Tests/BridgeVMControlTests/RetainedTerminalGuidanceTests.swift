import XCTest
@testable import BridgeVMControl

/// A terminal retained control keeps its slot until its record is closed. The refusals it
/// causes must name that action, and closing the record must release the slot at once,
/// not only after some view next looks the control up.
@MainActor
final class RetainedTerminalGuidanceTests: XCTestCase {
    func testUndismissedTerminalRecordNamesTheActionThatReleasesFileActions() throws {
        let f = try RetainedTerminalRecoveryFixture()
        let (current, _) = try f.replaceWhileObservedRunning()
        f.accepted.applyRuntimeStatus(running: false, ip: nil)
        for action in [LibraryFileAction.deletion, .clone, .move] {
            XCTAssertFalse(f.library.admitLibraryAction(current, action: action))
        }
        for refusal in [f.library.deletionError, f.library.cloneError, f.library.moveError] {
            XCTAssertTrue(refusal?.contains("'목록에서 닫기'") == true, refusal ?? "no refusal")
        }
        f.accepted.start()
        XCTAssertTrue(f.accepted.statusNote.contains("'목록에서 닫기'"), f.accepted.statusNote)
        XCTAssertFalse(f.accepted.lifecycleBusy)
    }

    func testDismissalReleasesTheSlotBeforeAnyLookup() throws {
        let f = try RetainedTerminalRecoveryFixture()
        let (current, record) = try f.replaceWhileObservedRunning()
        f.accepted.applyRuntimeStatus(running: false, ip: nil)
        try f.retainedView(record).dismiss()
        let slot = try XCTUnwrap(f.library.cachedControlModels[current.slug])
        XCTAssertFalse(slot === f.accepted, "Closing the record must release the retained control's slot")
        XCTAssertEqual(slot.config, current)
        XCTAssertFalse(slot.isRetained)
        XCTAssertTrue(f.library.admitLibraryAction(current, action: .deletion))
        XCTAssertNil(f.library.deletionError)
        XCTAssertTrue(f.library.model(for: current) === slot)
        XCTAssertEqual(f.factory.models.count, 2)
    }
}
