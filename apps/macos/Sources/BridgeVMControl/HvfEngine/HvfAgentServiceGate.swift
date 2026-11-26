import Foundation

/// Admission for lines appended to the agent control file. The relay rebuilds
/// its agent harness per boot generation and starts tailing the control file at
/// its current end, so a line appended before `BVAGENT SERVICE start` can be
/// skipped and its reply never arrives.
enum HvfAgentServiceGate {
    /// Guest commands come from the UI, so they wait (holding the command lock)
    /// only briefly for a boot that is about to finish, then refuse.
    static let guestCommandWait: TimeInterval = 10
    static let vmNotRunningMessage = "Start the VM before sending a command."
    static let serviceNotStartedMessage = "Wait for Guest Tools to start in Windows, then try the command again."

    /// Polls at the stop() cadence while the VM runs and the marker is absent.
    /// Returns true only once the service marker has been seen.
    static func waitForService(timeout: TimeInterval, started: () -> Bool, running: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while !started(), running(), Date() < deadline {
            usleep(250_000)
        }
        return started()
    }

    /// Nil once the service marker is seen, so a guest command may be appended;
    /// otherwise the refusal to report.
    static func guestCommandRefusal(waiting timeout: TimeInterval, started: () -> Bool, running: () -> Bool) -> String? {
        if waitForService(timeout: timeout, started: started, running: running) { return nil }
        return running() ? serviceNotStartedMessage : vmNotRunningMessage
    }
}
