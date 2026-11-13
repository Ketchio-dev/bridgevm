extension HvfEngineSession {
    func append(_ event: BvAgentEvent) {
        shareRootNotice.consume(event)
        diskSyncNotice.consume(event)
        agentUpdateNotice.consume(event)
        fileDrops.consume(event)
        clipboardImageNotice.consume(event)
        agentSilenceNotice.consume(event)
        events.append(HvfGuestLinks.feedEvent(event))
        if events.count > 500 { events.removeFirst(events.count - 500) }
    }
}
