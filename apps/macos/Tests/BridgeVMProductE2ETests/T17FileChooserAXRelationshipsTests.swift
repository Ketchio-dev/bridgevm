import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserAXRelationshipsTests: XCTestCase {
    func testAbsentAndEmptyRelationshipsRemainEmpty() throws {
        XCTAssertTrue(try T17FileChooserAXRelationships.decode(nil).isEmpty)
        XCTAssertTrue(try T17FileChooserAXRelationships.decode(NSArray()).isEmpty)
    }
    func testOnlyAccessibilityElementsAreAcceptedWithAliasesPreserved() throws {
        let node = AXUIElementCreateApplication(getpid())
        let result = try T17FileChooserAXRelationships.decode([node, node] as NSArray)
        XCTAssertEqual(result.count, 2)
        XCTAssertTrue(result.allSatisfy { CFEqual($0, node) })
    }
    func testNonArrayAndMixedElementPayloadsAreRejected() {
        let node = AXUIElementCreateApplication(getpid())
        for raw in [NSNumber(value: 1), NSString(string: "invalid"),
                    NSArray(array: [NSNumber(value: 1)]),
                    NSArray(array: [node, NSString(string: "invalid")])] {
            XCTAssertThrowsError(try T17FileChooserAXRelationships.decode(raw)) { error in
                XCTAssertEqual((error as? T17Blocker)?.detail, "chooser relationship payload was invalid")
            }
        }
    }
    func testMalformedLaterBranchCannotHideAnotherOwnerAndPermitPress() {
        let budget = T17ChooserNativeBudget(deadline: 1, now: { 0 })
        var reads: [Int] = [], metadata = 0, enabled = 0, presses = 0, pauses = 0
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget, lookup: {
            try T17FileChooserSelectionLookup.read(budget: budget, root: { 0 }, related: { node in
                reads.append(node)
                if node == 3 {
                    _ = try T17FileChooserAXRelationships.decode(NSNumber(value: 1))
                }
                return [0: [1, 3], 1: [2]][node] ?? []
            }, hash: { UInt($0) }, same: ==, role: { node in metadata += 1; return [0: "AXApplication", 1: "AXWindow", 2: "AXButton"][node] },
                identifier: { node in metadata += 1; return [1: "open-panel", 2: "OKButton"][node] }, pause: { pauses += 1 })
        }, enabled: { _ in enabled += 1; return true }, press: { _ in presses += 1 })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "chooser relationship payload was invalid")
        }
        XCTAssertEqual(reads, [0, 1, 3]); XCTAssertEqual(metadata, 0)
        XCTAssertEqual(pauses, 0); XCTAssertEqual(enabled, 0); XCTAssertEqual(presses, 0)
    }
}
