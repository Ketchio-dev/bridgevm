/// Values already retained by the session, not a new liveness or guest-health check.
struct HvfRuntimeObservation: Equatable {
    let connectionState: HvfConnectionState
    let ownedProcessIdentity: HvfOwnedRuntimeIdentity?
    let lastOwnedExit: HvfOwnedRuntimeExit?
    let hasRetainedAttachment: Bool
}

extension HvfEngineSession {
    func runtimeObservation() -> HvfRuntimeObservation {
        HvfRuntimeObservation(connectionState: connectionState,
            ownedProcessIdentity: ownedProcessIdentity, lastOwnedExit: lastOwnedExit,
            hasRetainedAttachment: hasRetainedAttachment)
    }
}
