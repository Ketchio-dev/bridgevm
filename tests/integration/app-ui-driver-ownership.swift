import Foundation

private func check(_ condition: @autoclosure () -> Bool, _ name: String) {
    if !condition() { fatalError("dual ownership contract: " + name) }
}
private func owner(_ role: AppUIHostV2Role) -> AppUIHostV2Ownership {
    AppUIHostV2Ownership(role: role, bundlePath: "/private/fixture/" + role.bundleName,
                        executableSHA256: String(repeating: "a", count: 64), requestedAt: 100)
}
private func identity(_ value: AppUIHostV2Ownership, pid: Int32 = 123, date: Double = 101) -> AppUIDriverProcessIdentity {
    AppUIDriverProcessIdentity(pid: pid, launchDate: date, bundleIdentifier: value.role.identifier,
        bundlePath: value.bundlePath, executablePath: value.executablePath,
        executableSHA256: value.executableSHA256)
}

@main
enum AppUIDriverOwnershipContracts {
    static func main() {
        var host = owner(.host), driver = owner(.driver)
        host.willLaunch(); driver.willLaunch()
        check(host.observe(identity(host), now: 1), "host admitted")
        check(driver.observe(identity(driver, pid: 124), now: 1), "driver admitted independently")
        check(host.poll(now: 2, exited: true) == .finish && host.cleanupVerified, "host observed exit")
        check(!driver.cleanupVerified && !driver.finished, "host exit does not release driver")
        driver.requestStop("cancelled", now: 3)
        check(driver.poll(now: 3, exited: false) == .terminate, "driver TERM immediately")
        check(driver.authorize(.terminate, current: driver.identity, now: 3) == 124, "exact driver only")
        check(driver.poll(now: 5.99, exited: false) == .wait, "three second grace retained")
        check(driver.poll(now: 6, exited: false) == .kill, "KILL deadline")
        check(driver.authorize(.kill, current: driver.identity, now: 6) == 124, "exact KILL target")
        check(driver.poll(now: 7, exited: true) == .finish && driver.cleanupVerified, "independent exit")
        check(!driver.success, "cancelled cleanup is not successful run")

        var absent = owner(.driver)
        absent.requestStop("prelaunch refusal", now: 1)
        check(absent.noLaunchVerified && absent.cleanupVerified && absent.finished, "proved not requested")
        check(!absent.success && absent.identity == nil, "no launch cannot pass scenario")
        var unresolved = owner(.driver)
        unresolved.willLaunch(); unresolved.requestStop("callback missing", now: 1)
        unresolved.requestStop("repeated cancel", now: 4)
        check(unresolved.poll(now: 5.99, exited: true) == .wait, "unowned exit signal proves nothing")
        check(unresolved.poll(now: 6, exited: true) == .finish, "first deadline is retained")
        check(!unresolved.cleanupVerified && !unresolved.noLaunchVerified, "uncertain launch fences")

        var reused = owner(.host)
        reused.willLaunch(); check(reused.observe(identity(reused), now: 1), "original host identity")
        reused.requestStop("cancelled", now: 2)
        check(reused.authorize(.terminate, current: identity(reused, date: 102), now: 2) == nil, "PID reuse refuses signal")
        check(!reused.cleanupVerified && reused.finished, "identity conflict fences")
        var late = owner(.driver)
        late.willLaunch(); late.requestStop("cancelled", now: 1)
        check(late.observe(identity(late), now: 4), "late callback still admitted for cleanup")
        check(late.poll(now: 4, exited: false) == .terminate, "late process terminated")
        check(late.authorize(.terminate, current: late.identity, now: 4) != nil, "fresh late identity required")
        check(late.poll(now: 6.99, exited: false) == .wait, "late grace counted once")

        var wrong = owner(.driver)
        wrong.willLaunch()
        check(!wrong.observe(identity(owner(.host)), now: 1), "wrong role rejected")
        check(!wrong.cleanupVerified, "wrong role does not grant ownership")
        var duplicate = owner(.host)
        duplicate.willLaunch(); check(duplicate.observe(identity(duplicate), now: 1), "first callback")
        check(!duplicate.observe(identity(duplicate, pid: 456), now: 2), "second process conflicts")
        _ = duplicate.poll(now: 3, exited: true)
        check(!duplicate.cleanupVerified, "one observed exit cannot clear ownership conflict")
        var ended = owner(.host)
        ended.willLaunch(); check(ended.observe(identity(ended), now: 1), "owned before exit")
        _ = ended.poll(now: 2, exited: true)
        check(!ended.observe(identity(ended), now: 3) && ended.cleanupVerified,
              "same complete identity late callback preserves observed exit")
        check(!ended.observe(identity(ended, pid: 456), now: 4) && !ended.cleanupVerified,
              "new process after owned exit still fences")
        print("app UI driver dual ownership: PASS (no processes or GUI launched)")
    }
}
