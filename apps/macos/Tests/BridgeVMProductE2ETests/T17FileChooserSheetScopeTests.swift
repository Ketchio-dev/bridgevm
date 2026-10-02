import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserSheetScopeTests: XCTestCase {
    private let panel = AXUIElementCreateApplication(101)
    private let sheet = AXUIElementCreateApplication(102)
    private let split = AXUIElementCreateApplication(103)

    func testCandidatesAreThePanelsDirectChildrenAndWindowsWithoutDuplicates() throws {
        var asked: [String] = []
        let result = try T17FileChooserSheetScope.candidates(of: panel) { node, name in
            XCTAssertTrue(CFEqual(node, self.panel)); asked.append(name)
            return name == kAXChildrenAttribute ? [split, sheet] : [sheet]
        }
        XCTAssertEqual(asked, T17AccessibilityTree.relationships)
        XCTAssertEqual(result.count, 2)
        XCTAssertTrue(CFEqual(result[0], split) && CFEqual(result[1], sheet))
    }
    func testAFailedPanelReadPropagatesSoAbsenceIsNeverInferred() {
        XCTAssertThrowsError(try T17FileChooserSheetScope.candidates(of: panel) { _, _ in
            throw T17FileChooser.failure("file chooser AXChildren read failed; ax_error=-25200")
        }) { XCTAssertTrue(T17FileChooserSnapshot.isTransientReadFailure($0)) }
    }
}
