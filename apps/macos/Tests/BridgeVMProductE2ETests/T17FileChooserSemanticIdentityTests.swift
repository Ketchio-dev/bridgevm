import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserSemanticIdentityTests: XCTestCase {
    private struct Node: Equatable {
        let identity: Int
        let id: String?
        let role: String?
    }

    private func find(_ nodes: [Node], id: String = "GoToWindow",
                      roles: Set<String> = ["AXSheet"]) throws -> Node? {
        try T17FileChooserSemanticIdentity.find(
            in: nodes, id: id, roles: roles,
            metadata: { ($0.id, $0.role) },
            same: { $0.identity == $1.identity }
        )
    }

    func testStableIdentifierWinsBeforeSemanticCandidate() throws {
        let semantic = Node(identity: 1, id: nil, role: "AXSheet")
        let identified = Node(identity: 2, id: "GoToWindow", role: "AXSheet")
        XCTAssertEqual(try find([semantic, identified]), identified)
    }

    func testUniqueRoleSupportsMissingOrChangedIdentifier() throws {
        for id: String? in [nil, "private-system-identifier"] {
            let candidate = Node(identity: 1, id: id, role: "AXSheet")
            XCTAssertEqual(try find([candidate]), candidate)
        }
    }

    func testFallbackIsScopedToAllowedRoles() throws {
        let button = Node(identity: 1, id: nil, role: "AXButton")
        XCTAssertNil(try find([button]))
    }

    func testDistinctSemanticCandidatesFailClosed() {
        let nodes = [Node(identity: 1, id: nil, role: "AXSheet"),
                     Node(identity: 2, id: "other", role: "AXSheet")]
        XCTAssertThrowsError(try find(nodes))
    }

    func testDuplicateReferencesToOneCandidateAreAccepted() throws {
        let node = Node(identity: 1, id: nil, role: "AXSheet")
        XCTAssertEqual(try find([node, node]), node)
    }

    func testIdentifiedWrongRoleStillFailsInsteadOfFallingBack() {
        let wrong = Node(identity: 1, id: "GoToWindow", role: "AXWindow")
        let fallback = Node(identity: 2, id: nil, role: "AXSheet")
        XCTAssertThrowsError(try find([wrong, fallback]))
    }

    func testUniquePathFieldUsesSameBoundedRule() throws {
        let field = Node(identity: 1, id: "private", role: "AXTextField")
        let button = Node(identity: 2, id: nil, role: "AXButton")
        XCTAssertEqual(try find([button, field], id: "PathTextField",
                                roles: ["AXTextField", "AXComboBox"]), field)
    }

    func testMetadataFailureIsRetained() {
        XCTAssertThrowsError(try T17FileChooserSemanticIdentity.find(
            in: [1], id: "GoToWindow", roles: ["AXSheet"],
            metadata: { _ in throw T17FileChooser.failure("fixture failure") },
            same: ==
        ))
    }
}
