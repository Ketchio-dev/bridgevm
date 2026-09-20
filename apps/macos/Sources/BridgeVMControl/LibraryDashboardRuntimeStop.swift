import Foundation

extension LibraryDashboardPrimaryAction {
    static func stop(
        session: HvfEngineSession?,
        requestStop: (HvfEngineSession, HvfOwnedRuntimeIdentity) -> HvfRuntimeStopOutcome,
        report: (String) -> Void,
        refresh: () -> Void
    ) {
        HvfRuntimeStopAction.perform(
            session: session, requestStop: requestStop, report: report, refresh: refresh)
    }
}
