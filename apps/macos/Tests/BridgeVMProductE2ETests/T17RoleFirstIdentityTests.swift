import XCTest
@testable import BridgeVMProductE2E

final class T17RoleFirstIdentityTests: XCTestCase {
    private let allowed = Set(["AXWindow", "AXSheet", "AXDialog"])

    func testDisallowedRoleSkipsIdentifierRead() throws {
        var reads = 0
        let role: (Int) throws -> String? = { $0 == 0 ? "AXButton" : "AXSheet" }
        let identifier: (Int) throws -> String? = { node in
            reads += 1
            if node == 0 { throw T17FileChooser.failure("irrelevant read") }
            return "open-panel"
        }
        let result = try T17RoleFirstIdentity.find(
            in: [0, 1], id: "open-panel", roles: allowed,
            role: role, identifier: identifier, same: ==
        )
        XCTAssertEqual(result, 1)
        XCTAssertEqual(reads, 1)
    }
    func testEligibleRoleRequiresExactIdentifier() throws {
        let result = try T17RoleFirstIdentity.find(
            in: [0, 1, 2], id: "open-panel", roles: allowed,
            role: { _ in "AXWindow" },
            identifier: { [nil, "other", "open-panel"][$0] }, same: ==
        )
        XCTAssertEqual(result, 2)
    }
    func testDuplicateReferenceIsAccepted() throws {
        let result = try T17RoleFirstIdentity.find(
            in: [1, 1], id: "open-panel", roles: allowed,
            role: { _ in "AXDialog" }, identifier: { _ in "open-panel" }, same: ==
        )
        XCTAssertEqual(result, 1)
    }

    func testDistinctEligibleMatchesAreRejected() {
        XCTAssertThrowsError(try T17RoleFirstIdentity.find(
            in: [1, 2], id: "open-panel", roles: allowed,
            role: { _ in "AXSheet" }, identifier: { _ in "open-panel" }, same: ==
        ))
    }

    func testRoleReadFailureIsNotAbsence() {
        XCTAssertThrowsError(try T17RoleFirstIdentity.find(
            in: [1], id: "open-panel", roles: allowed,
            role: { _ in throw T17FileChooser.failure("role read") },
            identifier: { _ in "open-panel" }, same: ==
        ))
    }

    func testEligibleIdentifierReadFailureIsNotAbsence() {
        XCTAssertThrowsError(try T17RoleFirstIdentity.find(
            in: [1], id: "open-panel", roles: allowed,
            role: { _ in "AXWindow" },
            identifier: { _ in throw T17FileChooser.failure("identifier read") }, same: ==
        ))
    }
}
