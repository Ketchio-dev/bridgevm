import Testing
@testable import BridgeVMProductE2E

struct T17RoleQualifiedIdentityTests {
    private struct ReadFailure: Error {}
    private struct Node: Equatable {
        let id: String?; let role: String?; let idFails: Bool; let roleFails: Bool
        init(id: String?, role: String?, idFails: Bool = false, roleFails: Bool = false) {
            self.id = id; self.role = role; self.idFails = idFails; self.roleFails = roleFails
        }
    }

    @Test func exactIdentifierAndRoleSelectOnlyTheEditableField() throws {
        let field = Node(id: "host", role: "AXTextField")
        let nodes = [Node(id: "host", role: "AXStaticText"), field,
                     Node(id: "host", role: "AXButton")]
        #expect(try find(nodes) == field)
    }

    @Test func missingExactRoleReturnsNil() throws {
        #expect(try find([Node(id: "host", role: "AXStaticText")]) == nil)
    }

    @Test func duplicateExactRolesFailClosed() {
        #expect(throws: T17Blocker.self) {
            try find([Node(id: "host", role: "AXTextField"),
                      Node(id: "host", role: "AXTextField")])
        }
    }

    @Test func metadataFailurePropagatesWhenNoMatchExists() {
        #expect(throws: ReadFailure.self) {
            try find([Node(id: nil, role: nil, idFails: true)])
        }
    }

    @Test func exactReadableMatchSurvivesUnrelatedMetadataFailure() throws {
        let field = Node(id: "host", role: "AXTextField")
        #expect(try find([Node(id: nil, role: nil, idFails: true), field]) == field)
    }

    @Test func roleFailureOnExactIdentifierPropagatesEvenWithAnotherMatch() {
        #expect(throws: ReadFailure.self) {
            try find([Node(id: "host", role: nil, roleFails: true),
                      Node(id: "host", role: "AXTextField")])
        }
    }

    private func find(_ nodes: [Node]) throws -> Node? {
        try T17RoleQualifiedIdentity.find("host", role: "AXTextField", in: nodes, identifier: {
            if $0.idFails { throw ReadFailure() }; return $0.id
        }, role: { if $0.roleFails { throw ReadFailure() }; return $0.role })
    }
}
