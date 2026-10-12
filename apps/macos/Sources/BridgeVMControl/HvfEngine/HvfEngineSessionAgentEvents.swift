import Foundation

extension HvfEngineSession {
    /// The control gate is per boot generation. After a guest reset record the next harness
    /// tails agent.ctl from its own start offset, so a line appended before that generation's
    /// `BVAGENT SERVICE start` can be skipped (AGENTS.md §6). `BVAGENT re-READY` restarts only
    /// the agent within the same harness and keeps the gate open.
    func handle(_ event: BvAgentEvent) {
        append(event)
        switch event {
        case let .ready(host, _):
            if connectionState != .stopping, ownedController?.failure == nil {
                connectionState = .connected(host: host)
            }
        case .serviceStart:
            serviceStarted = true
            if let ownedController { ownedController.guestServiceReady() }
            else { sendGracefulStopIfReady() }
        case let .unknown(line) where HvfGuestResetRecord.matches(line):
            serviceStarted = false
            ownedController?.guestServiceLost()
        case .aliveHeartbeat:
            lastHeartbeatDate = Date()
            lastHeartbeatAge = 0
        default:
            break
        }
    }
}
