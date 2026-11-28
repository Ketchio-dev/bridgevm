import SwiftUI

/// Opt-in durable disk writes: every Windows flush waits for F_FULLFSYNC. Only flushes change, not
/// every write, so the wording speaks of Windows asking to save its changes.
struct HvfDurableDiskWritesToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var explanation: String {
        isOn
            ? "Each time Windows asks to save its changes, it waits until they’re stored on your Mac’s disk. This is safest if your Mac loses power, but can slow Windows down."
            : "Windows doesn’t wait, and its saved changes are stored on your Mac’s disk within about a second. If your Mac loses power, Windows may lose about the last second of saved changes, but its disk stays intact."
    }

    var body: some View {
        Toggle("Wait until each save reaches your Mac’s disk", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.durable-disk-writes")
            .disabled(isDisabled)
            .help(explanation
                + (session.persistsLibraryPreferences ? " Changes made while the VM is stopped are saved for future starts." : ""))
            .alert("Disk write settings", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("OK") { message = nil }
            } message: {
                Text(message ?? "")
            }
        Text(explanation)
            .font(.caption)
            .foregroundStyle(.secondary)
    }

    private func update(_ enabled: Bool) {
        let result = session.applyDurableDiskWritesToggle(enabled)
        isOn = result.isOn
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyDurableDiskWritesToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsLibraryPreferences else { return (enabled, nil) }
        let message = saveLibraryPreference("Disk writes", publish: publish) {
            $0.durableDiskWritesEnabled = enabled ? true : nil
        }
        return (config.libraryContext?.config.durableDiskWritesEnabled ?? false, message)
    }
}
