extension LibraryModel {
    func bindControlModel(_ model: ControlModel) -> ControlModel {
        model.workAdmission = controlWorkAdmission(for: model)
        model.legacyStopAdmission = legacyStopAdmission(for: model)
        model.onConfigurationPersisted = { [weak self] in self?.reload() }
        model.onResourceUpdateFailed = { [weak self] in self?.reload() }
        return model
    }
}
