enum HvfRuntimeAccessibilityState {
    static func code(pending: Bool, connection: HvfConnectionState) -> String {
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
