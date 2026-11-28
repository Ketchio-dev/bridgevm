/// The end-to-end harness reads the runtime state and whether startup failed from
/// accessibility identifiers. VoiceOver never reads identifiers, so it reads only the
/// visible text.
enum HvfRuntimeAccessibilityState {
    static func identifier(pending: Bool, connection: HvfConnectionState) -> String {
        "bridgevm.windows.runtime.state." + code(pending: pending, connection: connection)
    }

    static func failureIdentifier(_ failure: String?) -> String {
        failure == nil ? "bridgevm.windows.runtime.start.failure.none" : "bridgevm.windows.runtime.start.failure"
    }

    private static func code(pending: Bool, connection: HvfConnectionState) -> String {
        if pending { return "start-pending" }
        switch connection {
        case .stopped: return "stopped"
        case .booting: return "booting"
        case .connected: return "connected"
        case .stopping: return "stopping"
        case .timedOut: return "timed-out"
        }
    }
}
