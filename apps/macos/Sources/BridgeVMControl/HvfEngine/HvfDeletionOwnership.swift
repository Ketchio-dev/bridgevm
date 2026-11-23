import Foundation

/// The pending path is for never-started registrations, not interrupted installs.
/// App action admission serializes same-app owners; this is not a script or
/// cross-library installation lock.
enum HvfDeletionOwnership {
    static func withOwnership<T>(config: VMConfig, rootURL: URL,
                                 operation: () throws -> T) throws -> T {
        let pending = config.installPending == true
        return try HvfMediaLeaseSession.withOwnership(
            config: config, mode: pending ? .pendingDelete : .ordinary
        ) {
            // READY owns the absent logical media paths. Check the saved registration
            // and residue once, right before removal; the operation checks references.
            if pending { try HvfPendingDeletionGuard.validate(config, rootURL: rootURL) }
            return try operation()
        }
    }
}
