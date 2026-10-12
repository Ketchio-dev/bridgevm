extension HvfRuntimeSessionStore {
    func isActive(slug: String) -> Bool {
        existingRecord(slug: slug)?.session.hasActiveRuntimeWork == true
    }

    func owns(_ session: HvfEngineSession, for config: VMConfig) -> Bool {
        guard let entry = existingRecord(slug: config.slug) else { return false }
        return entry.session === session && entry.sourceConfig == config
    }
}
