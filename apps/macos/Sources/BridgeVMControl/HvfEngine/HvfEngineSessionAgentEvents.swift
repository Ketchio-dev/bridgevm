import Foundation

extension HvfEngineSession {
    /// The control gate is per boot generation. After a guest reset record the next harness
    /// tails agent.ctl from its own start offset, so a line appended before that generation's
    /// `BVAGENT SERVICE start` can be skipped (AGENTS.md, guest agent). `BVAGENT re-READY` restarts only
    /// the agent within the same harness and keeps the gate open.
    func handle(_ event: BvAgentEvent) {
        append(event)
        switch event {
        case let .ready(host, _):
            guestStartApps = HvfGuestStartApps.advertised(host)
            if connectionState != .stopping, ownedController?.failure == nil {
                connectionState = .connected(host: host)
            }
        case .serviceStart:
            serviceStarted = true
            noteAgentServiceStarted()
            // A guest reboot recreates the helper, whose display device starts at its default size
            // and reads input.ctl from its end: poll() sends the window size again.
            displayResize.deviceRestarted()
            applyGuestHardwareCursorSetting()
            applyGuestLinksSetting()
            extendGrownDisk()
            if let ownedController { ownedController.guestServiceReady() }
            else { sendGracefulStopIfReady() }
        case let .unknown(line) where HvfGuestResetRecord.matches(line):
            serviceStarted = false
            guestBootChanged() // a pending Restart or Reset is done; an open confirmation is stale
            ownedController?.guestServiceLost()
        case let .unknown(line) where line.hasPrefix(Self.resetAcknowledgement):
            resetAcknowledged()
        case let .unknown(line) where line.hasPrefix("BVPAUSE "):
            pauseReported(line)
        case let .unknown(line) where line.hasPrefix("BVSTORAGE "):
            storageReported(line)
        case let .unknown(line) where line.hasPrefix(HvfGuestLinks.linePrefix):
            guestLinkReported(line)
        case let .unknown(line) where line.hasPrefix("BVAGENT re-READY "):
            // The agent restarted (a guest tools update): the new one says what it takes.
            guestStartApps = HvfGuestStartApps.advertised(line)
        case .aliveHeartbeat:
            lastHeartbeatDate = Date()
            lastHeartbeatAge = 0
        default:
            break
        }
    }
}
