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

    @Test func codedIdentifierMatchesOnlyItsOwnBase() {
        let state = Node(id: "bridgevm.windows.runtime.state.booting", fails: false)
        let failure = Node(id: "bridgevm.windows.runtime.start.failure.none", fails: false)
        let projection = T17IdentifierProjection.collect(
            ["bridgevm.windows.runtime.state", "bridgevm.windows.runtime.start"], in: [state, failure], identifier: { $0.id })
        #expect(projection.matches == ["bridgevm.windows.runtime.state": state])
    }

    @Test func missingExactTargetRetainsTheFirstReadFailure() {
        let nodes = [Node(id: nil, fails: true), Node(id: "other", fails: false)]
        #expect(throws: ReadFailure.self) { try find(nodes) }
    }

    private func find(_ nodes: [Node]) throws -> Node? {
        try T17CreationProbe.find(T17CreationProbe.installIdentifier, in: nodes, identifier: {
            if $0.fails { throw ReadFailure() }
            return $0.id
        })
    }
}
