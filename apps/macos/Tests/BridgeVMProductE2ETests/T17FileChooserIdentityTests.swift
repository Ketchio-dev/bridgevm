import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserIdentityTests: XCTestCase {
    private func find(_ nodes: [Int], metadata: (Int) throws -> (String?, String?)) throws -> Int? {
        try T17FileChooserIdentity.find(in: nodes, id: "PathTextField",
                                       roles: ["AXTextField", "AXComboBox"], metadata: metadata, same: ==)
    }

    func testUnrelatedTextFieldIsNotSelected() throws {
        let result = try find([0, 1]) { ($0 == 0 ? "unrelated" : "PathTextField", "AXTextField") }
        XCTAssertEqual(result, 1)
    }

    func testMissingIdentityDoesNotFallBackToRole() throws {
        XCTAssertNil(try find([0]) { _ in (nil, "AXTextField") })
    }

    func testDuplicateReferencesToSameObjectAreAccepted() throws {
        XCTAssertEqual(try find([1, 1]) { _ in ("PathTextField", "AXComboBox") }, 1)
    }

    func testDistinctMatchingObjectsAreRejected() {
        XCTAssertThrowsError(try find([1, 2]) { _ in ("PathTextField", "AXTextField") })
    }

    func testUnexpectedOrMissingRoleIsRejected() {
        for role: String? in ["AXButton", nil] {
            XCTAssertThrowsError(try find([1]) { _ in ("PathTextField", role) })
        }
    }

    func testMetadataFailureIsNotAbsence() {
        XCTAssertThrowsError(try find([1]) { _ in throw T17FileChooser.failure("fixture read failure") })
    }
}
