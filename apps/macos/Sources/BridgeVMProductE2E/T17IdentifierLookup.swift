import Foundation

/// Polls one exact identifier until the caller's deadline. An exhausted snapshot
/// that ended in a retryable AX read failure means "not available yet"; if the
/// final attempt ended that way, its attributed blocker replaces the missing capture.
enum T17IdentifierLookup {
    static func poll<Element>(
        _ identifier: String, timeout: TimeInterval, now: () -> Date = Date.init,
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) },
        snapshot: () throws -> Element?, missing: () -> Error
    ) throws -> Element {
        let deadline = now().addingTimeInterval(timeout)
        var transient: Error?
        repeat {
            do {
                if let match = try snapshot() { return match }
                transient = nil
            } catch where T17ApplicationSnapshotFailure.isRetryable(error) {
                transient = error
            } catch {
                throw T17ApplicationSnapshotFailure.attributed(error, identifier: identifier)
            }
            pause()
        } while now() < deadline
        if let transient { throw T17ApplicationSnapshotFailure.attributed(transient, identifier: identifier) }
        throw missing()
    }
}
