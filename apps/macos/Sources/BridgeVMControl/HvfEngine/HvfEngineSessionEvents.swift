extension HvfEngineSession {
    func append(_ event: BvAgentEvent) {
        shareRootNotice.consume(event)
        agentUpdateNotice.consume(event)
        fileDrops.consume(event)
        clipboardImageNotice.consume(event)
        events.append(event)
        if events.count > 500 { events.removeFirst(events.count - 500) }
    }
}
