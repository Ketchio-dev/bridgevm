import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserSelectionLookupDeadlineTests: XCTestCase {
    private func lookup(budget: T17ChooserNativeBudget, root: () -> Int,
                        related: (Int) throws -> [Int], role: (Int) throws -> String?,
                        identifier: (Int) throws -> String?, pause: () -> Void = {}) throws -> Int? {
        try T17FileChooserSelectionLookup.read(budget: budget, root: root, related: related,
            hash: { _ in 1 }, same: ==, role: role, identifier: identifier, pause: pause)
    }
    private func role(_ node: Int) -> String { ["AXApplication", "AXWindow", "AXButton"][node] }
    private func identifier(_ node: Int) -> String? { [nil, "open-panel", "OKButton"][node] }
    private func related(_ node: Int) -> [Int] { node < 2 ? [node + 1] : [] }

    func testExpiredLookupCreatesNoRootOrReadOrInput() {
        var roots = 0, reads = 0, enabled = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { 1 })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { roots += 1; return 0 }, related: { _ in reads += 1; return [] },
                role: { _ in reads += 1; return nil }, identifier: { _ in reads += 1; return nil })
        }, enabled: { _ in enabled += 1; return true }, press: { _ in presses += 1 }))
        XCTAssertEqual(roots, 0); XCTAssertEqual(reads, 0); XCTAssertEqual(enabled, 0); XCTAssertEqual(presses, 0)
    }
    func testLateReturnedRelationshipStopsGraphProjectionAndInput() {
        var time = 0.0, reads = 0, metadata = 0, enabled = 0, presses = 0, pauses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { 0 }, related: { node in
                try budget.read(.relationship) { reads += 1; time = 1; return self.related(node) }
            }, role: { _ in metadata += 1; return nil }, identifier: { _ in metadata += 1; return nil },
                pause: { pauses += 1 })
        }, enabled: { _ in enabled += 1; return true }, press: { _ in presses += 1 })) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("operation=selection-button-lookup"))
            XCTAssertTrue(detail.contains("operation=snapshot-attempt"))
            XCTAssertTrue(detail.contains("operation=ax-relationship; boundary=returned"))
        }
        XCTAssertEqual(reads, 1); XCTAssertEqual(metadata, 0); XCTAssertEqual(pauses, 0)
        XCTAssertEqual(enabled, 0); XCTAssertEqual(presses, 0)
    }
    func testLateAttributeNamesCannotReadValueOrIdentifierOrPress() {
        var time = 0.0, names = 0, values = 0, identifiers = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { 0 }, related: self.related, role: { node in
                _ = try budget.read(.attributeNames) { names += 1; time = 1; return ["AXRole"] }
                return try budget.read(.attributeValue) { values += 1; return self.role(node) }
            }, identifier: { _ in identifiers += 1; return nil })
        }, enabled: { _ in XCTFail("late metadata reached enabled"); return true }, press: { _ in presses += 1 })) { error in
            XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=ax-attribute-names; boundary=returned") == true)
        }
        XCTAssertEqual(names, 1); XCTAssertEqual(values, 0); XCTAssertEqual(identifiers, 0); XCTAssertEqual(presses, 0)
    }
    func testLateAttributeValueCannotReadNextRoleOrPress() {
        var time = 0.0, names = 0, values = 0, identifiers = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { 0 }, related: self.related, role: { node in
                _ = try budget.read(.attributeNames) { names += 1; return ["AXRole"] }
                return try budget.read(.attributeValue) { values += 1; time = 1; return self.role(node) }
            }, identifier: { _ in identifiers += 1; return nil })
        }, enabled: { _ in XCTFail("late role reached enabled"); return true }, press: { _ in presses += 1 })) { error in
            XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=ax-attribute-value; boundary=returned") == true)
        }
        XCTAssertEqual(names, 1); XCTAssertEqual(values, 1); XCTAssertEqual(identifiers, 0); XCTAssertEqual(presses, 0)
    }
    func testLateEligibleIdentifierCannotProceedToButtonOrEnabled() {
        var time = 0.0, identifiers = 0, enabled = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { 0 }, related: self.related,
                role: self.role, identifier: { node in
                    try budget.read(.attributeValue) { identifiers += 1; time = 1; return self.identifier(node) }
                })
        }, enabled: { _ in enabled += 1; return true }, press: { _ in presses += 1 }))
        XCTAssertEqual(identifiers, 1); XCTAssertEqual(enabled, 0); XCTAssertEqual(presses, 0)
    }
    func testLateEnabledReturnCannotPressFreshlyResolvedButton() {
        var time = 0.0, enabled = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { 0 }, related: self.related,
                role: self.role, identifier: self.identifier)
        }, enabled: { _ in enabled += 1; time = 1; return true }, press: { _ in presses += 1 })) { error in
            XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=selection-enabled; boundary=returned") == true)
        }
        XCTAssertEqual(enabled, 1); XCTAssertEqual(presses, 0)
    }
    func testRetryPauseExpirationCannotReacquireRootOrReusePartialGraph() {
        var time = 0.0, roots = 0, reads = 0, pauses = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { roots += 1; return 0 }, related: { _ -> [Int] in
                reads += 1; throw T17FileChooser.failure("file chooser AXChildren read failed; ax_error=\(AXError.cannotComplete.rawValue)")
            }, role: self.role, identifier: self.identifier, pause: { pauses += 1; time = 1 })
        }, enabled: { _ in XCTFail("expired retry reached enabled"); return true }, press: { _ in presses += 1 }))
        XCTAssertEqual(roots, 1); XCTAssertEqual(reads, 1); XCTAssertEqual(pauses, 1); XCTAssertEqual(presses, 0)
    }
    func testLateThrownRelationshipKeepsKnownAXCodeWithoutRetryOrPress() {
        var time = 0.0, roots = 0, reads = 0, pauses = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try self.lookup(budget: budget, root: { roots += 1; return 0 }, related: { _ -> [Int] in
                try budget.read(.relationship) {
                    reads += 1; time = 1
                    throw T17FileChooser.failure("file chooser AXChildren read failed; ax_error=\(AXError.cannotComplete.rawValue)")
                }
            }, role: self.role, identifier: self.identifier, pause: { pauses += 1 })
        }, enabled: { _ in XCTFail("late failed read reached enabled"); return true }, press: { _ in presses += 1 })) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("operation=selection-button-lookup"))
            XCTAssertTrue(detail.contains("operation=ax-relationship; boundary=returned"))
            XCTAssertTrue(detail.contains("ax_error=\(AXError.cannotComplete.rawValue)"))
        }
        XCTAssertEqual(roots, 1); XCTAssertEqual(reads, 1); XCTAssertEqual(pauses, 0); XCTAssertEqual(presses, 0)
    }
    func testFreshLookupStillPreservesOnceOnlyAdmittedLatePress() {
        for result in [AXError.success, .cannotComplete] {
            var time = 0.0, presses = 0
            let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
            XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
                try self.lookup(budget: budget, root: { 0 }, related: self.related,
                    role: self.role, identifier: self.identifier)
            }, enabled: { _ in true }, press: { _ in
                _ = try budget.input(.selectionPress) { presses += 1; time = 1; return result }
            })) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.contains("operation=selection-press; boundary=returned"))
                XCTAssertTrue(detail.contains("ax_error=\(result.rawValue)"))
            }
            XCTAssertEqual(presses, 1)
        }
    }
}
