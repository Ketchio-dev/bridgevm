import ApplicationServices
import Foundation

enum T17ApplicationSnapshot {
    static func read<Node, Snapshot>(
        attempts: Int = 3, pause: () -> Void,
        root: () -> Node, nodes: (Node) throws -> [Node], project: ([Node]) throws -> Snapshot
    ) throws -> Snapshot {
        try T17RetryingSnapshot.read(
            attempts: attempts, root: root,
            retryable: T17ApplicationSnapshotFailure.isRetryable, beforeRetry: pause
        ) { try project(nodes($0)) }
    }
}
