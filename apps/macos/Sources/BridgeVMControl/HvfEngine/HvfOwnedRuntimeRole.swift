struct HvfOwnedRuntimeRole {
    var live: HvfOwnedRuntimeChild?
    var spawned: UInt64 = 0
    var reaped: UInt64 = 0
    var last: HvfOwnedRuntimeChild?
    var summary: HvfOwnedRuntimeRoleSummary {
        HvfOwnedRuntimeRoleSummary(spawnedCount: spawned, reapedCount: reaped, last: last)
    }
    mutating func start(_ child: HvfOwnedRuntimeChild) throws {
        guard live == nil, spawned < UInt64.max else { throw HvfOwnedRuntimeProtocolError.invalidLifecycle }
        if child.role == "helper" {
            guard child.generation == spawned else { throw HvfOwnedRuntimeProtocolError.invalidLifecycle }
        } else if spawned != 0 { throw HvfOwnedRuntimeProtocolError.invalidLifecycle }
        live = child; spawned += 1
    }
    mutating func reap(_ child: HvfOwnedRuntimeChild) throws {
        guard let live, live.pid == child.pid, live.generation == child.generation,
              reaped < spawned else { throw HvfOwnedRuntimeProtocolError.invalidLifecycle }
        self.live = nil; reaped += 1; last = child
    }
}
