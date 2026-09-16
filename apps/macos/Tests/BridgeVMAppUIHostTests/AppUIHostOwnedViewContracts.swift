#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

extension AppUIHostContractTests {
    func assertOwnedViewObservationContracts() throws {
        try assertOwnedViewStageAndRefusal()
        try assertOwnedViewEntryBudget()
        try assertOwnedViewMetadataAndWrites()
    }

    func assertOwnedViewStageAndRefusal() throws {
        let root = AppUIHostAXNode(), owned = AppUIHostAXNode()
        let refused = AppUIHostAXNode(), unreachable = AppUIHostAXNode()
        root.children = [owned, refused]
        refused.children = [unreachable]
        refused.rejected = true
        var fieldReads = 0, childrenReads = 0, axReads = 0
        let dark = AppUIHostOwnedViewProjection.collect(root, stage: .darkDefault,
            owned: { !$0.rejected }, fields: { node in fieldReads += 1; node.inspections += 1; return [:] },
            children: { node in childrenReads += 1; return node.children.compactMap { $0 as? AppUIHostAXNode } },
            accessibility: { _, _ in axReads += 1; return [:] })
        XCTAssertEqual(fieldReads, 2)
        XCTAssertEqual(childrenReads, 2)
        XCTAssertEqual(axReads, 0)
        XCTAssertEqual(refused.inspections, 0)
        XCTAssertEqual(unreachable.inspections, 0)
        XCTAssertEqual(dark["node_count"] as? Int, 3)
        XCTAssertEqual(dark["owned_window_refusal_count"] as? Int, 1)
        XCTAssertNil(dark["full_protocol_rejection_count"])
        XCTAssertEqual(dark["ax_queries_enabled"] as? Bool, false)
        XCTAssertEqual((dark["ax_entry_budget"] as? [String: Any])?["inspected"] as? Int, 0)
        let rows = try axRows(dark)
        XCTAssertTrue(rows.allSatisfy { $0["accessibility"] == nil })
        XCTAssertEqual(rows[1]["same_owned_window"] as? Bool, false)
        XCTAssertEqual(rows[1]["children_read"] as? Bool, false)
        XCTAssertEqual(dark["node_limit"] as? Int, 256)
        XCTAssertEqual(dark["depth_limit"] as? Int, 32)
        XCTAssertEqual(dark["child_limit"] as? Int, 128)

        for detachAfterFields in [true, false] {
            let node = AppUIHostAXNode()
            var children = 0, accessibility = 0
            let result = AppUIHostOwnedViewProjection.collect(node, stage: .welcomeTimeout,
                owned: { !$0.rejected }, fields: { value in value.rejected = detachAfterFields; return [:] },
                children: { _ in children += 1; return [AppUIHostAXNode()] },
                accessibility: { value, _ in accessibility += 1; value.rejected = true; return [:] })
            XCTAssertEqual(accessibility, detachAfterFields ? 0 : 1)
            XCTAssertEqual(children, 0)
            XCTAssertEqual(result["owned_window_refusal_count"] as? Int, 1)
            XCTAssertEqual(result["node_count"] as? Int, 1)
        }
    }

    func assertOwnedViewEntryBudget() throws {
        let root = AppUIHostAXNode(), child = AppUIHostAXNode(), last = AppUIHostAXNode()
        root.children = [child]
        child.children = [last]
        var described = 0, calls = 0
        let result = AppUIHostOwnedViewProjection.collect(root, stage: .welcomeTimeout,
            owned: { _ in true }, fields: { _ in [:] },
            children: { $0.children.compactMap { $0 as? AppUIHostAXNode } }, accessibility: { node, budget in
                calls += 1
                let counts = node === root ? (80, 30) : node === child ? (12, 20) : (7, 9)
                let describe: (Int) -> [String: Any] = { _ in described += 1; return [:] }
                let ordinary = budget.project(Array(0..<counts.0), describe: describe)
                let navigation = budget.project(Array(0..<counts.1), describe: describe)
                return ["ordinary_children": ordinary, "navigation_children": navigation]
            })
        XCTAssertEqual(calls, 3)
        XCTAssertEqual(described, 128)
        let summary = try XCTUnwrap(result["ax_entry_budget"] as? [String: Any])
        XCTAssertEqual(summary["limit"] as? Int, 128)
        XCTAssertEqual(summary["inspected"] as? Int, 128)
        XCTAssertEqual(summary["omitted"] as? Int, 30)
        XCTAssertEqual(summary["exhausted"] as? Bool, true)
        XCTAssertEqual(summary["truncated"] as? Bool, true)
        let accessibility = try axRows(result).map { try XCTUnwrap($0["accessibility"] as? [String: Any]) }
        let ordinary = accessibility.compactMap { $0["ordinary_children"] as? [String: Any] }
        let navigation = accessibility.compactMap { $0["navigation_children"] as? [String: Any] }
        XCTAssertEqual(ordinary.compactMap { $0["raw_count"] as? Int }, [80, 12, 7])
        XCTAssertEqual(navigation.compactMap { $0["raw_count"] as? Int }, [30, 20, 9])
        XCTAssertEqual(ordinary.compactMap { $0["inspected_count"] as? Int }, [80, 12, 0])
        XCTAssertEqual(navigation.compactMap { $0["inspected_count"] as? Int }, [30, 6, 0])
        XCTAssertEqual(navigation.compactMap { $0["omitted_count"] as? Int }, [0, 14, 9])
    }
}
#endif
