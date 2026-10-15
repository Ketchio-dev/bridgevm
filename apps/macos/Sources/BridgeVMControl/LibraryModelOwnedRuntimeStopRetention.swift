import Foundation

extension LibraryModel {
    /// A fresh stop ID may join a retained record only while its run is still the VM's current owned state.
    /// A pending start, live replacement or attachment means an older completed run must not answer for the VM.
    func retainsOwnedStopTarget(_ request: NativeRuntimeControlRequest) -> Bool {
        guard let token = UUID(uuidString: request.target.runToken),
              let records = try? runtimeRecords(slug: request.vmID) else { return false }
        let identity = HvfOwnedRuntimeIdentity(token: token, processID: request.target.processID)
        let sessions = records.map { ($0.session.runtimeObservation(), $0.session.hasPendingRuntimeStart) }
        if sessions.contains(where: { $0.0.ownedProcessIdentity == identity }) { return true }
        guard !sessions.contains(where: { $0.1 || $0.0.ownedProcessIdentity != nil || $0.0.hasRetainedAttachment }) else {
            return false
        }
        return sessions.contains { $0.0.lastOwnedExit?.identity == identity }
    }
}
