import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

private final class SelectionLookupFixture {
    var edges = [0: [1], 1: [2], 2: []]
    var roles = [0: "AXApplication", 1: "AXWindow", 2: "AXButton"]
    var identifiers = [1: "open-panel", 2: "OKButton"]
    var roots = [0], rootCount = 0, pauses = 0
    var relationshipReads: [Int] = []
    var clock = 0.0, relationshipCost = 0.0
    var roleFailure: ((Int) throws -> Void)?
    var budget: T17ChooserNativeBudget { T17ChooserNativeBudget(deadline: 1, now: { self.clock }) }

    func related(_ node: Int) throws -> [Int] {
        try budget.read(.relationship) {
            relationshipReads.append(node); clock += relationshipCost
            return edges[node] ?? []
        }
    }
    func role(_ node: Int) throws -> String? {
        try budget.read(.attributeValue) { try roleFailure?(node); return roles[node] }
    }
    func identifier(_ node: Int) throws -> String? {
        try budget.read(.attributeValue) { identifiers[node] }
    }
    func root() -> Int {
        let result = roots[min(rootCount, roots.count - 1)]; rootCount += 1; return result
    }
    func lookup() throws -> Int? {
        try T17FileChooserSelectionLookup.read(budget: budget, root: root, related: related,
            hash: { _ in 1 }, same: ==, role: role, identifier: identifier,
            pause: { self.pauses += 1 })
    }
    private func nodes(_ root: Int) throws -> [Int] {
        try T17FileChooserGraph.walk(root: root, limit: 12_000, related: related, hash: { _ in 1 }, same: ==)
    }
    private func currentPanel() throws -> Int? {
        try budget.snapshot(root: root, nodes: nodes, project: { candidates in
            try T17RoleFirstIdentity.find(in: candidates, id: "open-panel",
                roles: [kAXWindowRole, kAXSheetRole, "AXDialog"], role: self.role,
                identifier: self.identifier, same: ==)
        }, pause: { self.pauses += 1 })
    }
    private func attribute(_ node: Int, _ name: String) throws -> AnyObject? {
        guard name == kAXIdentifierAttribute, let value = try identifier(node) else { return nil }
        return value as NSString
    }
    // Exact91 openButton body, with AXUIElement bound to an owned no-IPC node token.
    func legacyLookup() throws -> Int? {
        guard let current = try currentPanel() else { return nil }
        return try nodes(current).first { try attribute($0, kAXIdentifierAttribute) as? String == "OKButton" }
    }
}

final class T17FileChooserSelectionLookupTests: XCTestCase {
    func testExactOldBodyRepeatsReadsThatFreshCaptureReuses() throws {
        let old = SelectionLookupFixture(), fresh = SelectionLookupFixture()
        XCTAssertEqual(try old.legacyLookup(), 2)
        XCTAssertEqual(try fresh.lookup(), 2)
        XCTAssertEqual(old.relationshipReads, [0, 1, 2, 1, 2])
        XCTAssertEqual(fresh.relationshipReads, [0, 1, 2])
        XCTAssertEqual(old.rootCount, 1); XCTAssertEqual(fresh.rootCount, 1)
    }
    func testOldDuplicateReadConsumesSameBudgetBeforeNewLookupCanPress() throws {
        let old = SelectionLookupFixture(), fresh = SelectionLookupFixture()
        old.relationshipCost = 0.21; fresh.relationshipCost = 0.21
        var oldPresses: [Int] = [], freshPresses: [Int] = []
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: old.budget,
            lookup: old.legacyLookup, enabled: { _ in true }, press: { oldPresses.append($0) }))
        try T17ChooserSelectionAction.perform(budget: fresh.budget,
            lookup: fresh.lookup, enabled: { _ in true }, press: { freshPresses.append($0) })
        XCTAssertEqual(oldPresses, []); XCTAssertEqual(freshPresses, [2])
        XCTAssertEqual(old.relationshipReads, [0, 1, 2, 1, 2])
        XCTAssertEqual(fresh.relationshipReads, [0, 1, 2])
        XCTAssertEqual(fresh.clock, 0.63, accuracy: 0.000_001)
    }
    func testLaterDistinctOwnerIsRejectedBeforeEnabledOrPress() {
        let fixture = SelectionLookupFixture()
        fixture.edges = [0: [1, 3], 1: [2], 2: [], 3: [4], 4: []]
        fixture.roles[3] = "AXSheet"; fixture.roles[4] = "AXButton"
        fixture.identifiers[3] = "open-panel"; fixture.identifiers[4] = "OKButton"
        var enabled = 0, presses = 0
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: fixture.budget,
            lookup: fixture.lookup, enabled: { _ in enabled += 1; return true },
            press: { _ in presses += 1 })) { error in
                XCTAssertEqual((error as? T17Blocker)?.detail, "identified chooser element was ambiguous")
            }
        XCTAssertEqual(fixture.relationshipReads, [0, 1, 3, 2, 4])
        XCTAssertEqual(enabled, 0); XCTAssertEqual(presses, 0)
    }
    func testOwnedButtonWinsWithoutProjectingEscapedForeignButton() throws {
        let fixture = SelectionLookupFixture()
        fixture.edges = [0: [1, 3], 1: [0, 2], 2: [], 3: [4], 4: []]
        fixture.roles[3] = "AXWindow"; fixture.roles[4] = "AXButton"
        fixture.identifiers[4] = "OKButton"
        XCTAssertEqual(try fixture.lookup(), 2)
        XCTAssertEqual(fixture.relationshipReads, [0, 1, 3, 2, 4])
    }
    func testSharedButtonAndForeignOnlyButtonAreNeverPressed() throws {
        for edges in [[0: [1, 3], 1: [2], 2: [], 3: [2]], [0: [1, 3], 1: [0], 3: [2], 2: []]] {
            let fixture = SelectionLookupFixture(); fixture.edges = edges; fixture.roles[3] = "AXWindow"
            var enabled = 0, presses = 0
            XCTAssertFalse(try T17ChooserSelectionAction.perform(budget: fixture.budget,
                lookup: fixture.lookup, enabled: { _ in enabled += 1; return true },
                press: { _ in presses += 1 }))
            XCTAssertEqual(enabled, 0); XCTAssertEqual(presses, 0)
        }
    }
    func testWrongRoleAndDistinctOwnedButtonsDoNotBecomeFirstMatch() {
        let wrongRole = SelectionLookupFixture(); wrongRole.roles[2] = "AXTextField"
        XCTAssertNil(try wrongRole.lookup())
        let duplicate = SelectionLookupFixture()
        duplicate.edges[1] = [2, 3]; duplicate.edges[3] = []
        duplicate.roles[3] = "AXButton"; duplicate.identifiers[3] = "OKButton"
        XCTAssertThrowsError(try duplicate.lookup())
    }
    func testAcceptanceReacquiresReplacementWhileOldHandleStillReportsEnabled() throws {
        let fixture = SelectionLookupFixture()
        let ready = try fixture.lookup(); XCTAssertEqual(ready, 2)
        fixture.edges = [0: [3], 3: [4], 4: []]
        fixture.roles[3] = "AXDialog"; fixture.roles[4] = "AXButton"
        fixture.identifiers[3] = "open-panel"; fixture.identifiers[4] = "OKButton"
        let enabled = [2: true, 4: true]
        var enabledReads: [Int] = [], pressed: [Int] = []
        try T17ChooserSelectionAction.perform(budget: fixture.budget, lookup: fixture.lookup,
            enabled: { enabledReads.append($0); return enabled[$0] }, press: { pressed.append($0) })
        XCTAssertEqual(enabled[ready!], true)
        XCTAssertEqual(enabledReads, [4]); XCTAssertEqual(pressed, [4]); XCTAssertEqual(fixture.rootCount, 2)
        XCTAssertEqual(fixture.relationshipReads, [0, 1, 2, 0, 3, 4])
    }
    func testAcceptanceDoesNotReuseRemovedOwnerOrButton() throws {
        let fixture = SelectionLookupFixture(); XCTAssertEqual(try fixture.lookup(), 2)
        fixture.edges = [0: []]
        var enabledReads = 0, presses = 0
        XCTAssertFalse(try T17ChooserSelectionAction.perform(budget: fixture.budget,
            lookup: fixture.lookup, enabled: { _ in enabledReads += 1; return true },
            press: { _ in presses += 1 }))
        XCTAssertEqual(enabledReads, 0); XCTAssertEqual(presses, 0); XCTAssertEqual(fixture.rootCount, 2)
        XCTAssertEqual(fixture.relationshipReads, [0, 1, 2, 0])
    }
    func testTransientProjectionRetryReacquiresRootAndWholeGraph() throws {
        let fixture = SelectionLookupFixture()
        fixture.roots = [0, 10]; fixture.edges[10] = [11]; fixture.edges[11] = [12]; fixture.edges[12] = []
        fixture.roles[10] = "AXApplication"; fixture.roles[11] = "AXSheet"; fixture.roles[12] = "AXButton"
        fixture.identifiers[11] = "open-panel"; fixture.identifiers[12] = "OKButton"
        fixture.roleFailure = { node in
            if node == 1 { throw T17FileChooser.failure("file chooser AXRole read failed; ax_error=\(AXError.invalidUIElement.rawValue)") }
        }
        XCTAssertEqual(try fixture.lookup(), 12)
        XCTAssertEqual(fixture.rootCount, 2); XCTAssertEqual(fixture.pauses, 1)
        XCTAssertEqual(fixture.relationshipReads, [0, 1, 2, 10, 11, 12])
    }
    func testNonTransientRelationshipFailureIsPreservedAndNotRetried() {
        var roots = 0, pauses = 0
        let original = T17FileChooser.failure("owned relationship failure")
        XCTAssertThrowsError(try T17FileChooserSelectionLookup.read(
            budget: T17ChooserNativeBudget(deadline: 1, now: { 0 }), root: { roots += 1; return 0 },
            related: { _ -> [Int] in throw original }, hash: { _ in 1 }, same: ==,
            role: { _ in nil }, identifier: { _ in nil }, pause: { pauses += 1 })) { error in
                XCTAssertEqual((error as? T17Blocker)?.detail, original.detail)
            }
        XCTAssertEqual(roots, 1); XCTAssertEqual(pauses, 0)
    }
}
