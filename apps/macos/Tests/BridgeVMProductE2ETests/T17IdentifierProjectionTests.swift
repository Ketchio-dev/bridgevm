import Testing
@testable import BridgeVMProductE2E

struct T17IdentifierProjectionTests {
    private struct ReadFailure: Error, Equatable {}
    private struct Node: Equatable { let id: String?; let fails: Bool }

    @Test func exactReadableMatchSurvivesAnUnrelatedReadFailure() throws {
        let target = Node(id: T17CreationProbe.installIdentifier, fails: false)
        let nodes = [Node(id: nil, fails: true), target]
        #expect(try find(nodes) == target)
    }

    @Test func readableCreationErrorStillOverridesTheExactInstallView() {
        let nodes = [Node(id: nil, fails: true),
                     Node(id: T17CreationProbe.installIdentifier, fails: false),
                     Node(id: T17CreationProbe.errorIdentifier, fails: false)]
        #expect(throws: T17Blocker.self) { try find(nodes) }
    }

    @Test func missingExactTargetRetainsTheFirstReadFailure() {
        let nodes = [Node(id: nil, fails: true), Node(id: "other", fails: false)]
        #expect(throws: ReadFailure.self) { try find(nodes) }
    }

    private func find(_ nodes: [Node]) throws -> Node? {
        try T17CreationProbe.find(T17CreationProbe.installIdentifier, in: nodes, identifier: {
            if $0.fails { throw ReadFailure() }
            return $0.id
        }, value: { _ in "vm-materialization-failed" })
    }
}
