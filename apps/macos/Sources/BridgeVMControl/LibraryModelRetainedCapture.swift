import Foundation

extension LibraryModel {
    func captureRetainedControls(_ models: [ControlModel]) {
        let registered = Set(vms.map(\.slug))
        let generic = models.filter { model in
            model.config.engineKind != .hvfEngine && model.hasActiveRuntimeWork && !vms.contains(where: model.matchesRegisteredConfiguration)
        }.map { model -> LibraryRetainedControlDescriptor in
            model.retainForRecovery()
            return .generic(config: model.config, model: model)
        }
        retainedControlStore.capture(
            windowsInstallSessions.retainedControls(excludingSlugs: registered)
                + hvfRuntimeSessions.retainedControls(excludingSlugs: registered) + generic)
    }
}

@MainActor
enum LibraryControlModelReconciliation {
    static func reconcile(_ cache: [String: ControlModel], configurations: [String: VMConfig]) -> [String: ControlModel] {
        cache.filter { slug, model in
            guard let current = configurations[slug] else { return false }
            if model.isRetained { return model.config != current && model.hasActiveRuntimeWork }
            if model.config == current { return true }
            // Native-HVF compatibility handles retain their existing cache policy.
            return model.running || model.lifecycleBusy || model.busy
        }
    }
}
