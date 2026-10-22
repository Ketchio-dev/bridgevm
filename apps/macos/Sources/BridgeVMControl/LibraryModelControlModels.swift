extension LibraryModel {
    func hasAcceptedControlOperation(for slug: String) -> Bool { cachedControlModels[slug]?.hasAcceptedOperation == true || cachedControlModels[slug]?.ownedRuntime?.snapshot.phase == .stopping }
    func hasActiveOwnedControlRuntime(for slug: String) -> Bool { cachedControlModels[slug]?.ownedRuntime?.snapshot.isActive == true }
    func hasMismatchedCachedControlConfiguration(for cfg: VMConfig) -> Bool {
        cachedControlModels[cfg.slug].map { $0.config != cfg } ?? false
    }
    func ownsControlModel(_ model: ControlModel, for config: VMConfig) -> Bool {
        cachedControlModels[config.slug] === model && model.config == config
    }

    var activeCachedGenericConfigurations: [VMConfig] {
        cachedControlModels.values.filter { $0.config.engineKind != .hvfEngine && $0.hasActiveRuntimeWork }.map(\.config)
    }
}
