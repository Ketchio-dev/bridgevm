import Foundation

// Virtual monotonic times exercise the complete cleanup window without sleep,
// process creation, AppKit, or an application lifecycle.
enum AppUIHostLateOwnershipContracts {
    private static func require(_ condition: @autoclosure () -> Bool, _ reason: String) throws {
        if !condition() { throw NSError(domain: "AppUIHostLateOwnershipContracts", code: 1,
                                       userInfo: [NSLocalizedDescriptionKey: reason]) }
    }
    static func run(expectation: AppUIHostLaunchExpectation,
                    owner: AppUIHostProcessIdentity) throws -> AppUIHostLaunchOwnership {
        func fresh() -> AppUIHostLaunchOwnership { AppUIHostLaunchOwnership(expectation: expectation, startedAt: 0) }
        var late = fresh()
        late.requestStop(.cancelled, now: 1)
        try require(late.observe(owner, now: 2), "late matching owner was not retained for cleanup")
        try require(late.poll(now: 2, ownedProcessExited: false) == .terminate, "late owner was not stopped")
        try require(late.authorize(.terminate, current: owner, now: 2) == owner.pid, "verified TERM was refused")
        late.requestStop(.cancelled, now: 3)
        try require(late.poll(now: 4.99, ownedProcessExited: false) == .wait, "TERM grace was shortened")
        try require(late.poll(now: 5, ownedProcessExited: false) == .kill, "repeated cancellation extended TERM grace")
        try require(late.authorize(.kill, current: owner, now: 5) == owner.pid, "verified KILL was refused")
        try require(late.poll(now: 6, ownedProcessExited: true) == .finish, "owned cancelled exit did not finish")
        try require(late.cleanupVerified && !late.success && late.cancelled, "cancelled cleanup became a passing diagnostic")

        var slow = fresh()
        slow.requestStop(.cancelled, now: 0)
        try require(slow.poll(now: 4.89, ownedProcessExited: false) == .wait, "late-owner window was shortened")
        try require(slow.observe(owner, now: 4.9), "owner near the five-second boundary was lost")
        try require(slow.poll(now: 4.9, ownedProcessExited: false) == .terminate, "slow owner was not stopped")
        try require(slow.authorize(.terminate, current: owner, now: 4.9) == owner.pid, "slow-owner TERM was refused")
        try require(slow.poll(now: 6, ownedProcessExited: false) == .wait, "cleanup stopped at the old adapter limit")
        try require(slow.poll(now: 7.89, ownedProcessExited: false) == .wait, "slow-owner TERM grace was shortened")
        try require(slow.poll(now: 7.9, ownedProcessExited: false) == .kill, "slow-owner KILL did not follow full TERM grace")
        try require(slow.authorize(.kill, current: owner, now: 7.9) == owner.pid, "slow-owner KILL was refused")
        try require(slow.poll(now: 9.89, ownedProcessExited: false) == .wait, "slow-owner exit grace was shortened")
        var unseen = slow
        try require(unseen.poll(now: 9.9, ownedProcessExited: false) == .finish, "unobserved late exit was unbounded")
        try require(!unseen.cleanupVerified && !unseen.success, "unobserved late exit claimed cleanup")
        try require(slow.poll(now: 9.9, ownedProcessExited: true) == .finish, "observed late exit did not finish")
        try require(slow.cleanupVerified && slow.cancelled && !slow.success, "late cancelled cleanup became a pass")
        return late
    }
}
