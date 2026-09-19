import ApplicationServices

enum T17FileChooserSnapshot {
    static func read<Node, Snapshot>(
        attempts: Int = 3, root: () -> Node,
        nodes: (Node) throws -> [Node], project: ([Node]) throws -> Snapshot
    ) throws -> Snapshot {
        try T17RetryingSnapshot.read(attempts: attempts, root: root, retryable: isInvalidElement) {
            try project(nodes($0))
        }
    }

    private static func isInvalidElement(_ error: Error) -> Bool {
        let suffix = " read failed; ax_error=\(AXError.invalidUIElement.rawValue)"
        return (error as? T17Blocker)?.detail.hasSuffix(suffix) == true
    }
}
