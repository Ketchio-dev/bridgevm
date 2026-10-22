extension LibraryModel {
    /// A retained control stays the cached control while it is active and, once
    /// terminal, until its retained record is dismissed; only then does lookup
    /// build the current registration's ordinary control.
    func keepsCachedControl(_ model: ControlModel) -> Bool {
        !model.isRetained || model.hasActiveRuntimeWork
            || retainedControlRecords.contains { $0.descriptor.identity == .generic(ObjectIdentifier(model)) }
    }
}
