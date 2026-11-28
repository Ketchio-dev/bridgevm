import Foundation

@MainActor
enum HvfRuntimeStopAction {
    @discardableResult
    static func perform(
        session: HvfEngineSession?,
        requestStop: (HvfEngineSession, HvfOwnedRuntimeIdentity) -> HvfRuntimeStopOutcome,
        report: (String) -> Void,
        refresh: () -> Void
    ) -> Bool {
        guard let session else { report("Couldn’t find this VM. Press Command-K in the library window and choose Refresh Library, then try again."); return false }
        guard let identity = session.ownedProcessIdentity else {
            if session.hasRetainedAttachment {
                report("Stop the VM from the app or tool that started it.")
            } else if session.hasPendingRuntimeStart {
                report("Wait for startup to finish, then stop the VM.")
            } else {
                report("Couldn’t find a running VM started by this app.")
            }
            return false
        }
        let accepted: Bool
        switch requestStop(session, identity) {
        case .requested: report("Stop requested."); accepted = true
        case .alreadyStopping: report("The VM is already stopping."); accepted = true
        case .alreadyStopped: report("The VM has already stopped."); accepted = true
        case .notOwned: report("The running process changed. Check its status before trying to stop it again."); accepted = false
        }
        refresh()
        return accepted
    }
}
