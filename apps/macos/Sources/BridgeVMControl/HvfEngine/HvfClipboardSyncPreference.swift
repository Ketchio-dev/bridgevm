import Foundation

/// Clipboard sync is two-way, so a library VM's choice is saved in vm.json (see
/// HvfLibraryPreferenceSave) and every start path launches with it.
extension HvfEngineSession {
    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyClipboardSyncToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsLibraryPreferences else { return (enabled, nil) }
        let message = saveClipboardSyncPreference(enabled, publish: publish)
        return (config.libraryContext?.config.clipboardSyncEnabled ?? true, message)
    }

    /// Returns nil once vm.json holds `enabled`, otherwise the refusal or warning.
    /// `true` is saved as an absent key so re-enabled VMs match never-touched ones.
    func saveClipboardSyncPreference(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        saveLibraryPreference("클립보드 동기화 설정", publish: publish) { $0.clipboardSyncEnabled = enabled ? nil : false }
    }
}
