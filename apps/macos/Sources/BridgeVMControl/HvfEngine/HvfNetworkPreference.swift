import Foundation

/// A guest the user took offline stays offline on every start path: a library VM's
/// "Virtio net" choice is saved in vm.json (`networkEnabled`; absent means on).
extension HvfEngineSession {
    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyNetworkToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsLibraryPreferences else { return (enabled, nil) }
        let message = saveNetworkPreference(enabled, publish: publish)
        return (config.libraryContext?.config.networkEnabled ?? true, message)
    }

    /// Writes only when the effective value changes. VM creation saves an explicit `true`,
    /// so turning a created VM's network back on restores its original bytes.
    func saveNetworkPreference(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        saveLibraryPreference("네트워크 설정", publish: publish) {
            if ($0.networkEnabled ?? true) != enabled { $0.networkEnabled = enabled }
        }
    }
}
