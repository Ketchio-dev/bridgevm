import XCTest
@testable import BridgeVMProductE2E

private final class CapturedGraphReadError: Error {}

final class T17FileChooserCapturedGraphTests: XCTestCase {
    private func capture(_ edges: [Int: [Int]], limit: Int = 12_000,
                         hash: @escaping (Int) -> UInt = { UInt($0) }) throws -> T17FileChooserCapturedGraph<Int> {
        try T17FileChooserCapturedGraph.read(root: 0, limit: limit,
            related: { edges[$0] ?? [] }, hash: hash, same: ==)
    }

    func testEachDistinctNodeIsReadOnceAndOwnershipDoesNotRereadRelationships() throws {
        let edges = [0: [1, 2, 1], 1: [3, 3], 2: [4], 3: [1], 4: []]
        var reads: [Int: Int] = [:]
        let graph = try T17FileChooserCapturedGraph.read(root: 0, related: { node in
            reads[node, default: 0] += 1
            return edges[node] ?? []
        }, hash: { _ in 1 }, same: ==)
        XCTAssertEqual(graph.nodes, [0, 1, 2, 3, 4])
        XCTAssertEqual(try graph.ownedNodes(by: 1), [1, 3])
        XCTAssertEqual(try graph.ownedNodes(by: 2), [2, 4])
        XCTAssertEqual(try graph.ownedNodes(by: 1), [1, 3])
        XCTAssertEqual(reads, [0: 1, 1: 1, 2: 1, 3: 1, 4: 1])
    }

    func testCycleToApplicationFiltersOtherWindowButKeepsTheOwnersButton() throws {
        let graph = try capture([0: [1, 2], 1: [0, 3], 2: [4], 3: [], 4: []])
        XCTAssertEqual(graph.nodes, [0, 1, 2, 3, 4])
        XCTAssertEqual(try graph.ownedNodes(by: 1), [1, 3])
        XCTAssertEqual(try graph.ownedNodes(by: 2), [2, 4])
    }

    func testButtonSharedThroughAPathBypassingOwnerAndItsDescendantsAreExcluded() throws {
        let graph = try capture([0: [1, 2], 1: [3, 4], 2: [4], 3: [], 4: [5], 5: []])
        XCTAssertEqual(graph.nodes, [0, 1, 2, 3, 4, 5])
        XCTAssertEqual(try graph.ownedNodes(by: 1), [1, 3])
        XCTAssertEqual(try graph.ownedNodes(by: 2), [2])
    }

    func testDirectApplicationBypassExcludesSharedButtonEvenWithAliasesAndHashCollisions() throws {
        let graph = try capture([0: [1, 4, 1], 1: [3, 4, 3], 3: [1], 4: []], hash: { _ in 0 })
        XCTAssertEqual(graph.nodes, [0, 1, 4, 3])
        XCTAssertEqual(try graph.ownedNodes(by: 1), [1, 3])
    }

    func testNestedOwnerKeepsItsDescendantsAndExcludesItsAncestorAndSibling() throws {
        let graph = try capture([0: [1], 1: [2, 5], 2: [3], 3: [4, 1], 4: [], 5: []])
        XCTAssertEqual(graph.nodes, [0, 1, 2, 5, 3, 4])
        XCTAssertEqual(try graph.ownedNodes(by: 2), [2, 3, 4])
    }

    func testSeparateCapturesReadCurrentEdgesWithoutChangingTheEarlierCapture() throws {
        var edges = [0: [1, 2], 1: [3], 2: [], 3: []]
        var reads: [Int: Int] = [:]
        func read() throws -> T17FileChooserCapturedGraph<Int> {
            try T17FileChooserCapturedGraph.read(root: 0, related: { node in
                reads[node, default: 0] += 1
                return edges[node] ?? []
            }, hash: { UInt($0) }, same: ==)
        }
        let earlier = try read()
        edges[1] = []
        edges[2] = [3]
        let current = try read()
        XCTAssertEqual(try earlier.ownedNodes(by: 1), [1, 3])
        XCTAssertEqual(try current.ownedNodes(by: 1), [1])
        XCTAssertEqual(try current.ownedNodes(by: 2), [2, 3])
        XCTAssertEqual(reads, [0: 2, 1: 2, 2: 2, 3: 2])
    }

    func testFailedCapturePropagatesTheOriginalErrorWithoutReadingLaterNodes() {
        let original = CapturedGraphReadError()
        var reads: [Int] = []
        XCTAssertThrowsError(try T17FileChooserCapturedGraph.read(root: 0, related: { node in
            reads.append(node)
            if node == 1 { throw original }
            return node == 0 ? [1, 2] : []
        }, hash: { UInt($0) }, same: ==)) { error in
            XCTAssertTrue((error as? CapturedGraphReadError) === original)
        }
        XCTAssertEqual(reads, [0, 1])
    }

    func testExactLimitAllowsAliasesAndCyclesButRejectsAnotherDistinctNode() throws {
        let graph = try capture([0: [1, 1], 1: [0, 1]], limit: 2, hash: { _ in 0 })
        XCTAssertEqual(graph.nodes, [0, 1])
        XCTAssertEqual(try graph.ownedNodes(by: 1), [1])
        XCTAssertThrowsError(try capture([0: [1, 2]], limit: 2)) { error in
            XCTAssertEqual(error as? T17Blocker, T17FileChooser.failure("file chooser AX tree exceeded limit"))
        }
        XCTAssertThrowsError(try capture([:], limit: 0)) { error in
            XCTAssertEqual(error as? T17Blocker, T17FileChooser.failure("invalid AX graph limit"))
        }
    }

    func testDefaultLimitAcceptsTwelveThousandNodesAndRefusesTheNextDistinctNode() throws {
        let graph = try T17FileChooserCapturedGraph.read(root: 0,
            related: { $0 < 11_999 ? [$0 + 1] : [] }, hash: { UInt($0) }, same: ==)
        XCTAssertEqual(graph.nodes.count, 12_000)
        XCTAssertEqual(graph.nodes.first, 0)
        XCTAssertEqual(graph.nodes.last, 11_999)
        XCTAssertThrowsError(try T17FileChooserCapturedGraph.read(root: 0,
            related: { $0 < 12_000 ? [$0 + 1] : [] }, hash: { UInt($0) }, same: ==)) { error in
                XCTAssertEqual(error as? T17Blocker, T17FileChooser.failure("file chooser AX tree exceeded limit"))
            }
    }

    func testRootAsOwnerOwnsTheEntireCapturedGraph() throws {
        let graph = try capture([0: [1, 2], 1: [0, 3], 2: [3], 3: []], hash: { _ in 0 })
        XCTAssertEqual(try graph.ownedNodes(by: 0), graph.nodes)
    }

    func testUnknownOwnerFailsClosedRatherThanCreatingAPhantomOwner() throws {
        let graph = try capture([0: [1], 1: []], hash: { _ in 0 })
        XCTAssertThrowsError(try graph.ownedNodes(by: 9)) { error in
            XCTAssertEqual(error as? T17Blocker, T17FileChooser.failure("chooser snapshot relationship was absent"))
        }
    }
}
