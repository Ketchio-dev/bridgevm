import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ApplicationWalkScopeTests: XCTestCase {
    private let application = AXUIElementCreateApplication(getpid())
    func testApplicationRootIsWalkedThroughWindowsOnly() {
        XCTAssertTrue(T17ApplicationWalkScope.isApplication(application))
        XCTAssertEqual(T17ApplicationWalkScope.relationships(of: application, root: application), [kAXWindowsAttribute])
    }
    func testNonApplicationRootAndOtherNodesKeepChildrenAndWindows() {
        let systemWide = AXUIElementCreateSystemWide()
        XCTAssertFalse(T17ApplicationWalkScope.isApplication(systemWide))
        XCTAssertEqual(T17ApplicationWalkScope.relationships(of: systemWide, root: systemWide), T17AccessibilityTree.relationships)
        let other = AXUIElementCreateApplication(getpid() == 1 ? 2 : 1)
        XCTAssertEqual(T17ApplicationWalkScope.relationships(of: other, root: application), T17AccessibilityTree.relationships)
    }
}
