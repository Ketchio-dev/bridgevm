import ApplicationServices
import Darwin
import XCTest
@testable import BridgeVMProductE2E

/// Production AX reads against an application element whose pid does not exist.
/// Every read fails, trusted or not (-25204 on the development host both ways),
/// so the failing node's role is unreadable and only the status value varies.
final class T17AccessibilityTreeReadFailureTests: XCTestCase {
    private static let absentPid: pid_t = 99_999

    override func setUpWithError() throws {
        errno = 0
        XCTAssertEqual(kill(Self.absentPid, 0), -1)
        XCTAssertEqual(errno, ESRCH)
    }

    private func assertReadFailure(_ attribute: String, line: UInt = #line, _ read: () throws -> Any?) {
        XCTAssertThrowsError(try read(), line: line) { error in
            let detail = (error as? T17Blocker)?.detail ?? "not a blocker"
            let prefix = "ax_tree_read_failed;attribute=\(attribute);role=unreadable;ax_error="
            XCTAssertTrue(detail.hasPrefix(prefix), detail, line: line)
            XCTAssertNotEqual(Int32(detail.dropFirst(prefix.count)) ?? AXError.success.rawValue,
                              AXError.success.rawValue, detail, line: line)
        }
    }

    func testFailedTreeReadsCarryTheFailingNodeRoleField() {
        let absent = AXUIElementCreateApplication(Self.absentPid)
        for name in [kAXIdentifierAttribute, kAXRoleAttribute] {
            assertReadFailure(name) { try T17AccessibilityTree.attribute(absent, name) }
        }
        assertReadFailure(kAXChildrenAttribute) { try T17AccessibilityTree.nodes(absent, limit: 12_000) }
    }

    func testAFailedRoleReadIsNotRepeatedAndOtherFailuresReadTheRoleOnce() {
        var reads = 0
        XCTAssertEqual(T17AXReadFailure.detail(attribute: kAXRoleAttribute, status: .failure) {
            reads += 1; return "AXGroup"
        }, "ax_tree_read_failed;attribute=AXRole;role=unreadable;ax_error=-25200")
        XCTAssertEqual(reads, 0)
        XCTAssertEqual(T17AXReadFailure.detail(attribute: kAXIdentifierAttribute, status: .invalidUIElement) {
            reads += 1; return "AXTextField"
        }, "ax_tree_read_failed;attribute=AXIdentifier;role=AXTextField;ax_error=-25202")
        XCTAssertEqual(reads, 1)
    }
}
