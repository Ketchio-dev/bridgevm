extension LibraryModel {
    var selectedModel: ControlModel? {
        guard let id = selectedID, let cfg = vms.first(where: { $0.slug == id }) else { return nil }
        return model(for: cfg)
    }

    var selectedDetail: (config: VMConfig, model: ControlModel)? {
        guard let id = selectedID, let latest = vms.first(where: { $0.slug == id }) else { return nil }
        let model = model(for: latest)
        // Keep an active control handle's accepted configuration reachable.
        // Once idle, use current registration even before another cache reload.
        let config = model.running || model.lifecycleBusy || model.busy ? model.config : latest
        return (config, model)
    }
}
