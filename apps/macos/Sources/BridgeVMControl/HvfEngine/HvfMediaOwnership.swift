import Foundation

extension HvfMediaLeaseSession {
    private static func acquire(config: VMConfig, mode: Mode = .ordinary) throws -> HvfMediaLeaseSession {
        let root = HvfEngineSession.defaultRepoRoot()
        let helper = root.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        let media = finalMediaPaths(config)
        return try HvfMediaLeaseSession(executable: helper, disk: media.disk, vars: media.vars, mode: mode)
    }

    static func withOwnership<T>(config: VMConfig, mode: Mode = .ordinary, operation: () throws -> T) throws -> T {
        let lease = try acquire(config: config, mode: mode)
        defer { lease.abort() }
        let value = try operation()
        try lease.finish()
        return value
    }

    static func withCopyOwnership<T>(config: VMConfig, operation: (() -> Bool) -> T?) -> T? {
        guard let lease = try? acquire(config: config) else { return nil }
        return copyWhileOwned(session: lease, operation: operation)
    }

    /// The operation must validate the completed copy before publishing it.
    /// The source may resume after validation; subsequent work uses only the copy.
    static func copyWhileOwned<T>(session: HvfMediaLeaseSession, operation: (() -> Bool) -> T?) -> T? {
        defer { session.abort() }
        var verified = false
        let result = operation {
            guard !verified else { return false }
            do {
                try session.finish()
                verified = true
                return true
            } catch { return false }
        }
        return verified ? result : nil
    }
}
