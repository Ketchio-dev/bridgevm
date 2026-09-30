import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17AXReadFailureTests: XCTestCase {
    func testDetailNamesTheFailingNodeRoleBeforeTheAXError() {
        XCTAssertEqual(T17AXReadFailure.detail(attribute: "AXIdentifier", role: "AXScrollArea", status: .failure),
                       "ax_tree_read_failed;attribute=AXIdentifier;role=AXScrollArea;ax_error=-25200")
        XCTAssertEqual(T17AXReadFailure.detail(attribute: "AXChildren", role: nil, status: .invalidUIElement),
                       "ax_tree_read_failed;attribute=AXChildren;role=unreadable;ax_error=-25202")
    }

    func testRoleTokenIsABoundedAXConstantAndNeverFreeText() {
        XCTAssertEqual(T17AXReadFailure.token("AXTextField"), "AXTextField")
        for unsafe in ["/private/guest.iso", "AXGroup;secret=1", "AX Group", "Window", "",
                       "AX" + String(repeating: "x", count: 47)] {
            XCTAssertEqual(T17AXReadFailure.token(unsafe), "other")
        }
        XCTAssertEqual(T17AXReadFailure.token(nil), "unreadable")
    }

    func testRetryClassificationAndAttributionSurviveTheRoleField() {
        for status in [AXError.failure, .invalidUIElement] {
            let failure = T17Blocker(code: "ui-element-missing", detail: T17AXReadFailure.detail(
                attribute: "AXIdentifier", role: "AXGroup", status: status))
            XCTAssertTrue(T17ApplicationSnapshotFailure.isRetryable(failure))
        }
        let permanent = T17Blocker(code: "ui-element-missing", detail: T17AXReadFailure.detail(
            attribute: "AXIdentifier", role: "AXGroup", status: .cannotComplete))
        XCTAssertFalse(T17ApplicationSnapshotFailure.isRetryable(permanent))
        let longest = T17Blocker(code: "ui-element-missing", detail: T17AXReadFailure.detail(
            attribute: "AXAttributeNames", role: "AX" + String(repeating: "x", count: 46), status: .failure))
        let attributed = T17OptionalTextSnapshot.attributed(longest, identifiers: [
            "bridgevm.windows.runtime.start.failure", "bridgevm.windows.runtime.state"])
        XCTAssertLessThan((attributed as? T17Blocker)?.detail.utf8.count ?? .max, 513)
    }
}
