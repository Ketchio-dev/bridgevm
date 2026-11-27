import Foundation

extension HvfEngineSession {
    /// Written at the agent's first SERVICE start; distinguishes missing tools from a broken agent.
    var agentSeenMarker: URL { URL(fileURLWithPath: config.evidenceDir).appendingPathComponent(VMLibrary.agentSeenMarkerName) }

    /// From poll(): the agent notice for this moment (HvfAgentSilenceNotice).
    func refreshAgentSilence() {
        agentSilenceNotice.refresh(now: ownedStartNow(), running: agentSilenceWatched, serviceStarted: serviceStarted,
            firstConnection: { firstConnectionNotice }) {
            FileManager.default.fileExists(atPath: agentSeenMarker.path)
        }
    }

    private var firstConnectionNotice: HvfAgentSilenceNotice.Kind? {
        guard let vm = config.libraryContext?.config, vm.installPending != true else { return nil }
        let bundle = URL(fileURLWithPath: vm.bundlePath)
        let committed = bundle.appendingPathComponent(HvfWindowsInstallRequest.doneFileName)
        if FileManager.default.fileExists(atPath: committed.path) {
            return .awaitingFirstConnection
        }
        let pending = bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName)
        return FileManager.default.fileExists(atPath: pending.path) ? nil : .missingTools
    }

    func noteAgentServiceStarted() {
        guestToolsMessage = nil
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
