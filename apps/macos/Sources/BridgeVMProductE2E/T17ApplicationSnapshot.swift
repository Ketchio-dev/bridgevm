import ApplicationServices
import Foundation

enum T17ApplicationSnapshot {
    static func read<Node, Snapshot>(
        attempts: Int = 3, pause: () -> Void = { Thread.sleep(forTimeInterval: 0.2) },
        root: () -> Node, nodes: (Node) throws -> [Node], project: ([Node]) throws -> Snapshot
    ) throws -> Snapshot {
        try T17RetryingSnapshot.read(
            attempts: attempts, root: root, retryable: isInvalidElement, beforeRetry: pause
        ) { try project(nodes($0)) }
    }

    private static func isInvalidElement(_ error: Error) -> Bool {
        guard let blocker = error as? T17Blocker, blocker.code == "ui-element-missing" else { return false }
        return blocker.detail.hasPrefix("ax_tree_read_failed;")
            && blocker.detail.hasSuffix("ax_error=\(AXError.invalidUIElement.rawValue)")
    }
}
