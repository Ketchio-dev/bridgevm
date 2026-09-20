import XCTest
@testable import BridgeVMControl

final class LibraryDeletionActionTests: XCTestCase {
    func testRunningVMIsRefusedWithoutDeleting() {
        var deleteCalls = 0
        let outcome = LibraryDeletionAction.perform(isRunning: { true }) {
            deleteCalls += 1
            return true
        }

        XCTAssertEqual(outcome, .running)
        XCTAssertEqual(deleteCalls, 0)
    }

    func testStoppedVMReportsDeleteResult() {
        var livenessCalls = 0
        var deleteCalls = 0
        let deleted = LibraryDeletionAction.perform(isRunning: {
            livenessCalls += 1
            return false
        }, delete: {
            deleteCalls += 1
            return true
        })
        let failed = LibraryDeletionAction.perform(isRunning: { false }, delete: { false })

        XCTAssertEqual(deleted, .deleted)
        XCTAssertEqual(failed, .failed)
        XCTAssertEqual(livenessCalls, 1)
        XCTAssertEqual(deleteCalls, 1)
    }
}
