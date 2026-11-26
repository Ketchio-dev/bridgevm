import SwiftUI

/// Opt-in Retina resolution for the 2D virtio display (HvfDisplayResizeRequest). Only the app applies it,
/// but it is saved like the other per-VM preferences, which change only while the VM is fully stopped,
/// so it takes effect at the next start. Off again is saved as `false`, not removed: Windows keeps the
/// scale Retina mode set, so such a VM's requests keep naming 100 %.
struct HvfRetinaResolutionToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var setting: Bool?
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("Retina resolution", isOn: Binding(get: { setting == true }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.retina-resolution")
            .disabled(isDisabled)
            .help(Self.helpText(persists: session.persistsLibraryPreferences))
            .alert("Retina resolution", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("OK") { message = nil }
            } message: {
                Text(message ?? "")
            }
    }

    static func helpText(persists: Bool) -> String {
        "Experimental. On a Retina display, Windows uses the display’s native pixels and 200% scaling. This uses more CPU. Older apps may look blurry until reopened. Very large windows use standard resolution. After you turn this on once, BridgeVM adjusts Windows scaling whenever the window resizes, even if you later turn it off (100% outside Retina mode). This overrides custom Windows scaling."
            + (persists ? " Changes made while the VM is stopped are saved for future starts." : "")
    }

    /// On is `true`; off is `false` once it was set, and stays nil for a VM that never set it.
    static func saved(_ current: Bool?, enabled: Bool) -> Bool? {
        enabled ? true : current.map { _ in false }
    }

    private func update(_ enabled: Bool) {
        let result = session.applyRetinaResolutionToggle(enabled, current: setting)
        setting = result.setting
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the toggle holds afterwards: vm.json's value for a library VM, plus any message.
    func applyRetinaResolutionToggle(_ enabled: Bool, current: Bool? = nil,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (setting: Bool?, message: String?) {
        guard persistsLibraryPreferences else { return (HvfRetinaResolutionToggle.saved(current, enabled: enabled), nil) }
        let message = saveLibraryPreference("Retina resolution", publish: publish) {
            $0.retinaResolution = HvfRetinaResolutionToggle.saved($0.retinaResolution, enabled: enabled)
        }
        return (config.libraryContext?.config.retinaResolution, message)
    }
}
