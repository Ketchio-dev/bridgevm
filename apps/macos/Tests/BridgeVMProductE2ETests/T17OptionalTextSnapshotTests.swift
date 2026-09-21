import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17OptionalTextSnapshotTests: XCTestCase {
    private struct ReadFailure: Error, Equatable {}
    private struct Node { let id: String?; let value: String; let fails: Bool }
    private let state = "bridgevm.windows.runtime.state"
    private let failure = "bridgevm.windows.runtime.start.failure"

    func testProjectsBothValuesFromOneCompleteSnapshot() throws {
        let nodes = [Node(id: state, value: "booting", fails: false),
                     Node(id: failure, value: "private", fails: false)]
        XCTAssertEqual(try project(nodes), [state: "booting", failure: "private"])
    }

    func testCleanlyAbsentOptionalValueReturnsOnlyReadableMatch() throws {
        let nodes = [Node(id: state, value: "connected", fails: false),
                     Node(id: "other", value: "ignored", fails: false)]
        XCTAssertEqual(try project(nodes), [state: "connected"])
    }

    func testAmbiguousAbsenceRetainsIdentifierReadFailure() {
        let nodes = [Node(id: state, value: "booting", fails: false),
                     Node(id: nil, value: "", fails: true)]
        XCTAssertThrowsError(try project(nodes)) { XCTAssertTrue($0 is ReadFailure) }
    }

    func testUnrelatedReadFailureDoesNotOverrideTwoExactReadableValues() throws {
        let nodes = [Node(id: nil, value: "", fails: true),
                     Node(id: state, value: "booting", fails: false),
                     Node(id: failure, value: "private", fails: false)]
        XCTAssertEqual(try project(nodes), [state: "booting", failure: "private"])
    }

    func testGenericAXFailureReacquiresFiveCompleteSnapshots() throws {
        var roots = 0, pauses = 0
        let values = try T17OptionalTextSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return Node(id: nil, value: String(roots), fails: false)
        }, nodes: { root in
            if root.value != "5" { throw self.axFailure(.failure) }
            return [Node(id: self.state, value: "connected", fails: false)]
        }, expected: [state, failure], identifier: { $0.id }, text: { $0.value })
        XCTAssertEqual(values, [state: "connected"])
        XCTAssertEqual(roots, 5); XCTAssertEqual(pauses, 4)
    }

    func testNonRetryableAXFailureStopsAfterOneSnapshot() {
        var roots = 0, pauses = 0
        XCTAssertThrowsError(try T17OptionalTextSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return Node(id: nil, value: "", fails: false)
        }, nodes: { (_: Node) -> [Node] in throw self.axFailure(.cannotComplete) }, expected: [state, failure],
            identifier: { (node: Node) in node.id }, text: { $0.value }))
        XCTAssertEqual(roots, 1); XCTAssertEqual(pauses, 0)
    }

    func testFiveAttemptExhaustionRetainsFinalAXFailure() {
        var roots = 0, pauses = 0
        XCTAssertThrowsError(try T17OptionalTextSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return Node(id: nil, value: String(roots), fails: false)
        }, nodes: { root -> [Node] in throw self.axFailure(root.value == "5" ? .failure : .invalidUIElement) },
            expected: [state, failure], identifier: { $0.id }, text: { $0.value })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail,
                           "ax_tree_read_failed;attribute=AXIdentifier;ax_error=\(AXError.failure.rawValue)")
        }
        XCTAssertEqual(roots, 5); XCTAssertEqual(pauses, 4)
    }

    func testAttributionRetainsFailureAndSortedIdentifiers() {
        let error = T17OptionalTextSnapshot.attributed(axFailure(.failure), identifiers: [state, failure])
        XCTAssertEqual((error as? T17Blocker)?.detail,
                       "ax_tree_read_failed;attribute=AXIdentifier;ax_error=-25200;stage=identifier-search;identifiers=bridgevm.windows.runtime.start.failure,bridgevm.windows.runtime.state")
    }

    private func project(_ nodes: [Node]) throws -> [String: String] {
        try T17OptionalTextSnapshot.project([state, failure], in: nodes, identifier: {
            if $0.fails { throw ReadFailure() }; return $0.id
        }, text: { $0.value })
    }

    private func axFailure(_ error: AXError) -> T17Blocker {
        T17Blocker(code: "ui-element-missing",
                   detail: "ax_tree_read_failed;attribute=AXIdentifier;ax_error=\(error.rawValue)")
    }
}
