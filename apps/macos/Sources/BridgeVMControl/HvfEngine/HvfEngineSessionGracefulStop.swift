import Foundation

extension HvfEngineSession {
    /// Asks a connected guest to shut down once a stop is pending, or ends the wrapper when it cannot.
    /// A legacy wrapper launch has no Force Stop control, so its 180 s deadline still ends the wrapper.
    func sendGracefulStopIfReady() {
        guard case .stopping = connectionState, serviceStarted, !stopCommandSent else { return }
        stopCommandSent = true
        if sendCtl(HvfGuestStopCommand.shutdown.line) {
            append(.unknown("graceful guest shutdown requested"))
            return
        }
        append(.unknown("graceful guest shutdown unavailable; terminating the HVF wrapper"))
        if let process {
            process.terminate()
        } else if attachedToExistingProcess {
            Shell.killProcesses(matching: config.targetDiskPath)
        }
        stopDeadline = nil
    }
}
