import Foundation

@main
enum AppUIHostLauncherOwnershipContracts {
    static func main() throws {
        let path = "/owned/app-ui-private/BridgeVMAppUIHost.app"
        let hash = String(repeating: "a", count: 64)
        let expected = AppUIHostLaunchExpectation(bundlePath: path, executableSHA256: hash, requestedAt: 100)
        func process(pid: Int32 = 42, date: Double = 101, bundle: String? = nil,
                     executable: String? = nil, digest: String? = nil, identifier: String = "dev.bridgevm.app-ui-host") -> AppUIHostProcessIdentity {
            let bundlePath = bundle ?? path
            return AppUIHostProcessIdentity(pid: pid, launchDate: date, bundleIdentifier: identifier, bundlePath: bundlePath,
                executablePath: executable ?? (bundlePath + "/Contents/MacOS/BridgeVMControl"), executableSHA256: digest ?? hash)
        }
        func fresh() -> AppUIHostLaunchOwnership { AppUIHostLaunchOwnership(expectation: expected, startedAt: 0) }
        func require(_ condition: @autoclosure () -> Bool, _ reason: String) throws {
            if !condition() { throw NSError(domain: "AppUIHostLauncherContracts", code: 1,
                                           userInfo: [NSLocalizedDescriptionKey: reason]) }
        }
        let owner = process()
        for mismatch in [process(pid: 1), process(date: 99), process(date: .nan),
                         process(bundle: "/other/BridgeVMAppUIHost.app"), process(executable: "/other/BridgeVMControl"),
                         process(digest: String(repeating: "b", count: 64)), process(identifier: "dev.bridgevm.control")] {
            var state = fresh()
            try require(!state.observe(mismatch, now: 1), "mismatched ownership was accepted")
            try require(state.identity == nil && !state.cleanupVerified, "mismatch claimed ownership or cleanup")
        }
        var normal = fresh()
        try require(normal.observe(owner, now: 1), "matching identity was refused")
        try require(normal.poll(now: 2, ownedProcessExited: false) == .wait, "live normal host did not wait")
        try require(normal.poll(now: 3, ownedProcessExited: true) == .finish, "normal host exit did not finish")
        try require(normal.success && normal.cleanupVerified, "verified normal lifecycle did not pass")

        var unresolved = fresh()
        unresolved.requestStop(.cancelled, now: 1)
        try require(unresolved.poll(now: 5.9, ownedProcessExited: true) == .wait, "unowned exit supplied false cleanup")
        try require(unresolved.poll(now: 6, ownedProcessExited: false) == .finish, "unresolved cancellation was unbounded")
        try require(!unresolved.success && !unresolved.cleanupVerified, "unresolved cancellation claimed cleanup")
        try require(!unresolved.observe(owner, now: 7), "finished unresolved state adopted a late owner")

        let late = try AppUIHostLateOwnershipContracts.run(expectation: expected, owner: owner)

        var reused = fresh()
        _ = reused.observe(owner, now: 1)
        reused.requestStop(.cancelled, now: 2)
        try require(reused.authorize(.terminate, current: process(date: 102), now: 2) == nil,
                    "PID reuse with a new launch date was signalled")
        try require(reused.finished && !reused.cleanupVerified, "changed ownership was not refused")

        var escalation = fresh()
        _ = escalation.observe(owner, now: 1)
        escalation.requestStop(.cancelled, now: 2)
        _ = escalation.authorize(.terminate, current: owner, now: 2)
        try require(escalation.authorize(.kill, current: process(digest: String(repeating: "c", count: 64)), now: 5) == nil,
                    "KILL used a stale executable identity")

        var early = fresh()
        _ = early.observe(owner, now: 1)
        early.requestStop(.cancelled, now: 2)
        _ = early.authorize(.terminate, current: owner, now: 2)
        try require(early.authorize(.kill, current: owner, now: 4.99) == nil,
                    "force termination bypassed the full three-second TERM allowance")

        var timeout = fresh()
        _ = timeout.observe(owner, now: 1)
        try require(timeout.poll(now: 89.99, ownedProcessExited: false) == .wait, "deadline was shortened")
        try require(timeout.poll(now: 90, ownedProcessExited: false) == .terminate, "deadline did not stop owned host")
        _ = timeout.authorize(.terminate, current: owner, now: 90)
        _ = timeout.authorize(.kill, current: owner, now: 93)
        try require(timeout.poll(now: 94.99, ownedProcessExited: false) == .wait, "KILL grace was shortened")
        try require(timeout.poll(now: 95, ownedProcessExited: false) == .finish, "KILL grace did not bound execution")
        try require(timeout.timedOut && !timeout.exitObserved && !timeout.cleanupVerified && !timeout.success,
                    "unobserved exit was called successful cleanup")

        var duplicate = fresh()
        _ = duplicate.observe(owner, now: 1)
        try require(!duplicate.observe(process(pid: 43), now: 2), "second instance silently replaced the owner")
        _ = duplicate.poll(now: 3, ownedProcessExited: true)
        try require(!duplicate.cleanupVerified && !duplicate.success, "second unresolved instance claimed cleanup")
        let report = try JSONSerialization.data(withJSONObject: late.receipt)
        let keys: Set<String> = ["schema_version", "kind", "host_pid", "host_launch_date", "host_bundle_path",
            "host_executable_path", "host_executable_sha256", "identity_verified", "exit_observed", "cleanup_verified",
            "cancelled", "timed_out", "termination", "success", "failure"]
        try require(!report.isEmpty && Set(late.receipt.keys) == keys, "receipt schema changed")
        print("app-ui launcher ownership contracts PASS (pure state; no app or guest launch)")
    }
}
