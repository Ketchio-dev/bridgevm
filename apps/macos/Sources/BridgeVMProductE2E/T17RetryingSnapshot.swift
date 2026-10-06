enum T17RetryingSnapshot {
    static func read<Node, Snapshot>(
        attempts: Int, root: () -> Node, beforeRead: () throws -> Void = {},
        retryable: (Error) -> Bool, beforeRetry: () throws -> Void = {},
        snapshot: (Node) throws -> Snapshot
    ) throws -> Snapshot {
        guard attempts > 0 else {
            throw T17FileChooser.failure("invalid AX snapshot attempt count")
        }
        var lastError: Error?
        for attempt in 0..<attempts {
            do { try beforeRead(); return try snapshot(root()) }
            catch {
                guard retryable(error) else { throw error }
                lastError = error; if attempt + 1 < attempts { try beforeRetry() }
            }
        }
        throw lastError!
    }
}
