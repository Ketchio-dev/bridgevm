import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserScopeTests: XCTestCase {
    private let application = AXUIElementCreateApplication(getpid())
    func testApplicationRootIsWalkedThroughWindowsOnly() {
        XCTAssertTrue(T17FileChooserScope.isApplication(application))
        XCTAssertEqual(T17FileChooserScope.relationships(of: application, root: application), [kAXWindowsAttribute])
    }
    func testNonApplicationRootAndOtherNodesKeepChildrenAndWindows() {
        let systemWide = AXUIElementCreateSystemWide()
        XCTAssertFalse(T17FileChooserScope.isApplication(systemWide))
        XCTAssertEqual(T17FileChooserScope.relationships(of: systemWide, root: systemWide), T17FileChooserAXTree.relationships)
        let other = AXUIElementCreateApplication(getpid() == 1 ? 2 : 1)
        XCTAssertEqual(T17FileChooserScope.relationships(of: other, root: application), T17FileChooserAXTree.relationships)
    }
}
