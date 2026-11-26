import SwiftUI

/// Opt-in durable disk writes: every Windows flush waits for F_FULLFSYNC.
struct HvfDurableDiskWritesToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("Wait for durable disk flushes", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.durable-disk-writes")
            .disabled(isDisabled)
            .help("By default, disk writes stay ordered, but a sudden loss of power can lose about the last second of writes. Turn this on to wait for permanent storage before acknowledging each Windows disk flush. Writes can be slower on the internal SSD."
                + (session.persistsLibraryPreferences ? " Changes made while the VM is stopped are saved for future starts." : ""))
            .alert("Disk write settings", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("OK") { message = nil }
            } message: {
                Text(message ?? "")
            }
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
