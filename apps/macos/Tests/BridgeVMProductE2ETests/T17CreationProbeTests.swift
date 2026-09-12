import ApplicationServices
import Testing
@testable import BridgeVMProductE2E

struct T17CreationProbeTests {
    struct Node: Equatable { let id: String?; let value: String? }
    private func find(_ nodes: [Node], requested: String = T17CreationProbe.installIdentifier) throws -> Node? {
        try T17CreationProbe.find(requested, in: nodes, identifier: { $0.id }, value: { $0.value })
    }

    @Test func generalTreeHasBothPublicRelationships() {
        #expect(Set(T17AccessibilityTree.relationships) == Set([kAXChildrenAttribute, kAXWindowsAttribute]))
    }

    @Test func onlyExactRequiredIdentifierEstablishesSuccess() throws {
        let install = Node(id: T17CreationProbe.installIdentifier, value: nil)
        #expect(try find([Node(id: "bridgevm.install.start", value: nil)]) == nil)
        #expect(try find([install]) == install)
    }

    @Test func allKnownCreationFailuresAreBounded() {
        for code in T17CreationProbe.failureCodes {
            do {
                _ = try find([Node(id: T17CreationProbe.errorIdentifier, value: code)])
                Issue.record("creation error was ignored")
            } catch let error as T17Blocker {
                #expect(error.code == "vm-creation-failed")
                #expect(error.detail == "stage=create;reason=\(code)")
            } catch { Issue.record("unexpected error") }
        }
    }

    @Test func unknownPrivateValueNeverEscapes() {
        for value in [nil, "/private/user/path secret"] {
            do {
                _ = try find([Node(id: T17CreationProbe.errorIdentifier, value: value)])
                Issue.record("creation error was ignored")
            } catch let error as T17Blocker {
                #expect(error.detail == "stage=create;reason=creation-error-unclassified")
            } catch { Issue.record("unexpected error") }
        }
    }

    @Test func visibleErrorCannotBeHiddenByAnInstallNode() {
        #expect(throws: (any Error).self) {
            try find([Node(id: T17CreationProbe.installIdentifier, value: nil),
                      Node(id: T17CreationProbe.errorIdentifier, value: "vm-materialization-failed")])
        }
    }

    @Test func unrelatedQueriesDoNotReadErrorValues() throws {
        let node = Node(id: "other", value: nil)
        let result = try T17CreationProbe.find("other", in: [node], identifier: { $0.id }, value: { _ in
            Issue.record("unrelated query read error value"); return "private"
        })
        #expect(result == node)
    }

    @Test func metadataErrorsPropagate() {
        struct ReadFailure: Error {}
        #expect(throws: ReadFailure.self) {
            try T17CreationProbe.find("other", in: [0], identifier: { _ in throw ReadFailure() }, value: { _ in nil })
        }
    }
}
