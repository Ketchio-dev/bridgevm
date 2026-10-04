import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserNativeSnapshotTests: XCTestCase {
    private var transient: T17Blocker { T17FileChooser.failure("file chooser AXChildren read failed; ax_error=\(AXError.cannotComplete.rawValue)") }
    func testLateFailedReadDoesNotPauseOrCreateAnotherRoot() {
        var time = 0.0, roots = 0, pauses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try budget.snapshot(root: { roots += 1; return roots },
            nodes: { _ -> [Int] in time = 1; throw self.transient }, project: { $0 },
            pause: { pauses += 1 })) { error in
                XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=snapshot-retry-pause") == true)
            }
        XCTAssertEqual(roots, 1); XCTAssertEqual(pauses, 0)
    }
    func testPauseCrossingDeadlineDoesNotCreateAnotherRoot() {
        var time = 0.0, roots = 0, pauses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try budget.snapshot(root: { roots += 1; return roots },
            nodes: { _ -> [Int] in throw self.transient }, project: { $0 },
            pause: { pauses += 1; time = 1 }))
        XCTAssertEqual(roots, 1); XCTAssertEqual(pauses, 1)
    }
    func testTransientReadRecoversUnderSameDeadlineAndOwner() throws {
        var time = 0.0, roots = 0, reads = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        let owner = 7
        let value = try budget.snapshot(root: { roots += 1; return owner }, nodes: { node in
            reads += 1; if reads == 1 { throw self.transient }; return [node]
        }, project: { $0.first }, pause: { time += 0.2 })
        XCTAssertEqual(value, owner); XCTAssertEqual(roots, 2); XCTAssertEqual(time, 0.2)
    }
    func testLateRelationshipReadCannotStartAnotherRead() {
        var time = 0.0, relationships = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17FileChooserGraph.walk(root: 0, limit: 10, related: { node in
            try budget.read(.relationship) { relationships += 1; time = 1; return [node + 1] }
        }, hash: { UInt($0) }, same: ==))
        XCTAssertEqual(relationships, 1)
    }
    func testExpiredSnapshotCannotCreateEvenItsFirstRoot() {
        let budget = T17ChooserNativeBudget(deadline: 1, now: { 1 })
        XCTAssertThrowsError(try budget.snapshot(root: { XCTFail("late root"); return 7 },
            nodes: { [$0] }, project: { $0 }))
    }
    func testDeadlineRefusalDoesNotStartNodeLabelAXReads() {
        var labels = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { 1 })
        XCTAssertThrowsError(try T17FileChooserNodeLabel.naming(7,
            describe: { _ in labels += 1; return "owned" }, { try budget.check(.relationship) }))
        XCTAssertEqual(labels, 0)
    }
}
