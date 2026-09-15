#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

extension AppUIHostContractTests {
    func assertAllIncompleteHostObservationContracts() async throws {
        try assertIncompleteObservationCompletion()
        try await assertPresentationMatrixOrderAndMismatchLatch()
        try await assertPresentationMatrixFatalBoundaries()
        try await assertPresentationMatrixCancellation()
        try await assertPresentationMatrixPersistenceFailures()
        try await assertPresentationMatrixTypedCallbackFailures()
        try assertAXWalkBounds()
        try assertAXProtocolAndIdentifierObservations()
        try await assertAXTimeoutPreservesResult()
    }

    func assertAXWalkBounds() throws {
        let root = AppUIHostAXNode(), child = AppUIHostAXNode()
        root.children = [child, child]
        child.children = [root]
        defer { root.children = []; child.children = [] }
        let cycle = axWalk(root)
        XCTAssertEqual(cycle["node_count"] as? Int, 2)
        XCTAssertEqual(cycle["duplicate_count"] as? Int, 2)
        XCTAssertEqual(root.inspections, 1)
        XCTAssertEqual(child.inspections, 1)
        XCTAssertEqual(cycle["truncated"] as? Bool, false)

        let deep = AppUIHostAXNode()
        var tail = deep
        for _ in 0..<35 { let next = AppUIHostAXNode(); tail.children = [next]; tail = next }
        let depth = axWalk(deep)
        XCTAssertEqual(depth["node_count"] as? Int, 33)
        XCTAssertEqual(try axRows(depth).compactMap { $0["depth"] as? Int }.max(), 32)
        XCTAssertEqual(depth["depth_truncated"] as? Bool, true)
        XCTAssertEqual(depth["children_omitted_by_depth_limit"] as? Int, 1)
        XCTAssertEqual(tail.inspections, 0)

        let wide = AppUIHostAXNode()
        wide.children = (0..<130).map { _ in AppUIHostAXNode() }
        let width = axWalk(wide)
        XCTAssertEqual(width["node_count"] as? Int, 129)
        XCTAssertEqual(width["child_limit"] as? Int, 128)
        XCTAssertEqual(width["child_truncated"] as? Bool, true)
        XCTAssertEqual(width["children_omitted_by_child_limit"] as? Int, 2)
        XCTAssertEqual(try axRows(width).first?["child_count"] as? Int, 130)
        XCTAssertEqual(try axRows(width).first?["children_queued"] as? Int, 128)

        let broad = AppUIHostAXNode()
        broad.children = (0..<128).map { _ in
            let branch = AppUIHostAXNode()
            branch.children = [AppUIHostAXNode(), AppUIHostAXNode()]
            return branch
        }
        let bounded = axWalk(broad)
        XCTAssertEqual(bounded["node_count"] as? Int, 256)
        XCTAssertEqual(bounded["node_limit"] as? Int, 256)
        XCTAssertEqual(bounded["node_truncated"] as? Bool, true)
        XCTAssertLessThanOrEqual(try XCTUnwrap(bounded["maximum_pending"] as? Int), 256)
        XCTAssertGreaterThan(try XCTUnwrap(bounded["children_omitted_by_node_limit"] as? Int), 0)
        let rejected = AppUIHostAXNode(), unseen = AppUIHostAXNode()
        rejected.rejected = true
        rejected.children = [unseen]
        let refusal = axWalk(rejected)
        XCTAssertEqual(refusal["full_protocol_rejection_count"] as? Int, 1)
        XCTAssertEqual(refusal["node_count"] as? Int, 1)
        XCTAssertTrue(try axRows(refusal)[0]["child_count"] is NSNull)
        XCTAssertEqual(unseen.inspections, 0)
    }

    func assertAXProtocolAndIdentifierObservations() throws {
        let root = AppUIHostAXButtonFixture(), create = AppUIHostAXButtonFixture(), importButton = AppUIHostAXButtonFixture()
        create.identifier = "bridgevm.first-run.create"
        importButton.identifier = "bridgevm.first-run.import"
        create.role = "AXButton"
        root.children = [create, importButton]
        let strict = AppUIHostAXObservation.graph(root, publicSelectors: false)
        XCTAssertEqual(strict["node_count"] as? Int, 1)
        XCTAssertEqual(root.childrenReads, 0)
        let strictRoot = try axRows(strict)[0]
        XCTAssertEqual(strictRoot["conforms_element_protocol"] as? Bool, true)
        XCTAssertEqual(strictRoot["conforms_button_protocol"] as? Bool, true)
        XCTAssertEqual(strictRoot["conforms_full_protocol"] as? Bool, false)
        XCTAssertEqual(strictRoot["existing_walker_rejection"] as? String, "not-full-protocol")
        let observed = AppUIHostAXObservation.graph(root, publicSelectors: true)
        XCTAssertEqual(observed["node_count"] as? Int, 3)
        XCTAssertEqual(observed["full_protocol_rejection_count"] as? Int, 3)
        XCTAssertEqual(root.childrenReads, 1)
        XCTAssertEqual(observed["route"] as? String, "responding-public-getters-observation-only")
        XCTAssertEqual(observed["allowed_identifier_counts"] as? [String: Int],
                       ["bridgevm.first-run.create": 1, "bridgevm.first-run.import": 1])
        XCTAssertEqual(try axRows(observed).compactMap { $0["allowed_role"] as? String }, ["AXButton"])
        let encoded = try JSONSerialization.data(withJSONObject: observed)
        let text = String(decoding: encoded, as: UTF8.self)
        for secret in [root.identifier, root.role, "private-label", "private-value", "private-title"] {
            XCTAssertFalse(text.contains(secret))
        }
        XCTAssertEqual([root, create, importButton].map { $0.forbiddenReads }, [0, 0, 0])
        XCTAssertTrue(try axRows(observed).allSatisfy { (($0["type"] as? String)?.count ?? 129) <= 128 })
    }

    func assertAXTimeoutPreservesResult() async throws {
        var observations = 0
        try await AppUIHostAccessibility.wait("welcome controls", onTimeout: { observations += 1 }) { true }
        XCTAssertEqual(observations, 0)
        do {
            try await AppUIHostAccessibility.wait("welcome controls", onTimeout: { observations += 1 }) {
                throw AppUIHostMatrixFixture.Failure.ordinary
            }
            XCTFail("Predicate error unexpectedly passed")
        } catch { XCTAssertEqual(error as? AppUIHostMatrixFixture.Failure, .ordinary) }
        XCTAssertEqual(observations, 0)
        let cancelled = await Task { @MainActor in
            withUnsafeCurrentTask { $0?.cancel() }
            do {
                try await AppUIHostAccessibility.wait("welcome controls", onTimeout: { observations += 1 }) { false }
                return false
            } catch is CancellationError { return true }
            catch { return false }
        }.value
        XCTAssertTrue(cancelled)
        XCTAssertEqual(observations, 0)
        assertWelcomeTimeout {
            try AppUIHostAccessibility.timeout("welcome controls", onTimeout: { observations += 1 })
        }
        XCTAssertEqual(observations, 1)
        let fixture = try fixture(), capture = try AppUIHostCapture(output: fixture.output)
        let retained: [String: Any] = ["fixture": "retained first observation"]
        try AppUIHostAXObservation.write(retained, capture: capture)
        let path = fixture.output.appendingPathComponent(AppUIHostAXObservation.filename)
        let before = try Data(contentsOf: path)
        assertWelcomeTimeout {
            try AppUIHostAccessibility.timeout("welcome controls", onTimeout: {
                observations += 1
                try AppUIHostAXObservation.write(["replacement": true], capture: capture)
            })
        }
        XCTAssertEqual(observations, 2)
        XCTAssertEqual(try Data(contentsOf: path), before)
        XCTAssertEqual(try object(fixture.output, "ui-observations.json")["failure"] is NSNull, true)
    }
}
#endif
