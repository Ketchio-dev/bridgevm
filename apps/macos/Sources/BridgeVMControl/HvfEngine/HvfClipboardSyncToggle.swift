import SwiftUI

struct HvfClipboardSyncToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("Share clipboard", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.clipboard")
            .disabled(isDisabled)
            .help(session.persistsLibraryPreferences ? "Changes made while the VM is stopped are saved for future starts." : "")
            .alert("Clipboard sharing", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("OK") { message = nil }
            } message: {
                Text(message ?? "")
            }
    }

    private func update(_ enabled: Bool) {
        let result = session.applyClipboardSyncToggle(enabled)
        isOn = result.isOn
        message = result.message
    }
}
