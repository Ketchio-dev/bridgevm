import Foundation
import ApplicationServices
enum T17FileChooserSnapshot {
    static func read<Node, Snapshot>(
        attempts: Int = 10, pause: () -> Void = { Thread.sleep(forTimeInterval: 0.2) }, root: () -> Node,
        nodes: (Node) throws -> [Node], project: ([Node]) throws -> Snapshot
    ) throws -> Snapshot {
        try T17RetryingSnapshot.read(attempts: attempts, root: root,
            retryable: isTransientReadFailure, beforeRetry: pause) { try project(nodes($0)) }
    }
    static func isTransientReadFailure(_ error: Error) -> Bool {
        guard let detail = (error as? T17Blocker)?.detail else { return false }
        return [AXError.failure, .invalidUIElement, .cannotComplete]
            .contains { detail.hasSuffix(" read failed; ax_error=\($0.rawValue)") }
    }
}
