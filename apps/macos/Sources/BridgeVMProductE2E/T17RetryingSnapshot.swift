enum T17RetryingSnapshot {
    static func read<Node, Snapshot>(
        attempts: Int, root: () -> Node,
        retryable: (Error) -> Bool,
        snapshot: (Node) throws -> Snapshot
    ) throws -> Snapshot {
        guard attempts > 0 else {
            throw T17FileChooser.failure("invalid AX snapshot attempt count")
        }
        var lastError: Error?
        for _ in 0..<attempts {
            do { return try snapshot(root()) }
            catch {
                guard retryable(error) else { throw error }
                lastError = error
            }
        }
        throw lastError!
    }
}
