#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class AppUIHostAXNode {
    var children: [Any] = []
    var inspections = 0
    var rejected = false
}

@MainActor
final class AppUIHostAXButtonFixture: NSObject, NSAccessibilityButton {
    var children: [Any] = []
    var identifier = "not-allowed-private-identifier"
    var role = "not-allowed-private-role"
    var childrenReads = 0
    var forbiddenReads = 0
    func accessibilityFrame() -> NSRect { forbiddenReads += 1; return .zero }
    func accessibilityParent() -> Any? { forbiddenReads += 1; return nil }
    func accessibilityLabel() -> String? { forbiddenReads += 1; return "private-label" }
    func accessibilityPerformPress() -> Bool { forbiddenReads += 1; return false }
    @objc func accessibilityIdentifier() -> String { identifier }
    @objc func accessibilityRole() -> String { role }
    @objc func accessibilityChildren() -> [Any]? { childrenReads += 1; return children }
    @objc func accessibilityValue() -> Any? { forbiddenReads += 1; return "private-value" }
    @objc func accessibilityTitle() -> String? { forbiddenReads += 1; return "private-title" }
}

extension AppUIHostContractTests {
    func axWalk(_ root: AppUIHostAXNode) -> [String: Any] {
        AppUIHostAXWalk.collect(root, identity: { value in
            (value as? AppUIHostAXNode).map { ObjectIdentifier($0) }
        }, inspect: { value in
            guard let node = value as? AppUIHostAXNode else {
                return .init(fields: [:], children: nil, rejected: true)
            }
            node.inspections += 1
            return .init(fields: [:], children: node.rejected ? nil : node.children, rejected: node.rejected)
        })
    }
    func axRows(_ graph: [String: Any]) throws -> [[String: Any]] {
        try XCTUnwrap(graph["nodes"] as? [[String: Any]])
    }
    func assertWelcomeTimeout(_ action: () throws -> Void, file: StaticString = #filePath, line: UInt = #line) {
        do { try action(); XCTFail("Expected the original welcome timeout", file: file, line: line) }
        catch AppUIHostError.refused(let reason) {
            XCTAssertEqual(reason, "Timed out waiting for actual UI: welcome controls", file: file, line: line)
        } catch { XCTFail("Replaced the original welcome timeout", file: file, line: line) }
    }
}
#endif
