import Foundation

extension HvfEngineSession {
    /// Written at the agent's first `SERVICE start` in this VM. A VM without it (Windows setup's first
    /// logon still to come, or a disk imported without the agent) never shows the not-connected notice.
    var agentSeenMarker: URL { URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("guest-agent-seen") }

    /// From poll(): the agent notice for this moment (HvfAgentSilenceNotice).
    func refreshAgentSilence() {
        agentSilenceNotice.refresh(now: ownedStartNow(), running: agentSilenceWatched, serviceStarted: serviceStarted) {
            FileManager.default.fileExists(atPath: agentSeenMarker.path)
        }
    }

    func noteAgentServiceStarted() {
        let path = agentSeenMarker.path
        if !FileManager.default.fileExists(atPath: path) { _ = FileManager.default.createFile(atPath: path, contents: nil) }
    }

    /// Paused, stopping, or restarting (Windows goes down on purpose): quiet, and the wait for the
    /// agent starts again when the VM runs on.
    private var agentSilenceWatched: Bool {
        guard pause == .running, restartRequest == nil, !guiStartCleanupOnly else { return false }
        switch connectionState {
        case .booting, .connected: return true
        case .stopped, .stopping, .timedOut: return false
        }
    }
}
