import ApplicationServices
import Testing
@testable import BridgeVMProductE2E

struct T17CreationProbeTests {
    struct Node: Equatable { let id: String? }
    private func find(_ nodes: [Node], requested: String = T17CreationProbe.installIdentifier) throws -> Node? {
        try T17CreationProbe.find(requested, in: nodes, identifier: { $0.id })
    }
    private func error(_ code: String) -> Node { Node(id: "\(T17CreationProbe.errorIdentifier).\(code)") }

    @Test func generalTreeHasBothPublicRelationships() {
        #expect(Set(T17AccessibilityTree.relationships) == Set([kAXChildrenAttribute, kAXWindowsAttribute]))
    }

    @Test func onlyExactRequiredIdentifierEstablishesSuccess() throws {
        let install = Node(id: T17CreationProbe.installIdentifier)
        #expect(try find([Node(id: "bridgevm.install.start")]) == nil)
        #expect(try find([install]) == install)
    }

    @Test func allKnownCreationFailuresAreReadFromTheIdentifier() {
        for code in T17CreationProbe.failureCodes {
            do {
                _ = try find([error(code)])
                Issue.record("creation error was ignored")
            } catch let error as T17Blocker {
                #expect(error.code == "vm-creation-failed")
                #expect(error.detail == "stage=create;reason=\(code)")
            } catch { Issue.record("unexpected error") }
        }
    }

    @Test func uncodedOrUnknownErrorNeverEscapes() {
        for node in [Node(id: T17CreationProbe.errorIdentifier), error("/private/user/path secret")] {
            do {
                _ = try find([node])
                Issue.record("creation error was ignored")
            } catch let error as T17Blocker {
                #expect(error.detail == "stage=create;reason=creation-error-unclassified")
            } catch { Issue.record("unexpected error") }
        }
    }

    @Test func visibleErrorCannotBeHiddenByAnInstallNode() {
        #expect(throws: (any Error).self) {
            try find([Node(id: T17CreationProbe.installIdentifier), error("vm-materialization-failed")])
        }
    }

    @Test func unrelatedQueriesIgnoreAVisibleError() throws {
        let node = Node(id: "other")
        #expect(try find([error("hvf-import-invalid"), node], requested: "other") == node)
    }

    @Test func metadataErrorsPropagate() {
        struct ReadFailure: Error {}
        #expect(throws: ReadFailure.self) {
            try T17CreationProbe.find("other", in: [0], identifier: { _ in throw ReadFailure() })
        }
    }
}
