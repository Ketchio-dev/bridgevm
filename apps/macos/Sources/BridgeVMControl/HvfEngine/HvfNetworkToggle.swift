import SwiftUI

struct HvfNetworkToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("Enable networking", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.network")
            .disabled(isDisabled)
            .help(session.persistsLibraryPreferences ? "Changes apply the next time this VM starts." : "")
            .alert("Network settings", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("OK") { message = nil }
            } message: {
                Text(message ?? "")
            }
    }

    /// The toggle's action, also exercised by value tests without SwiftUI's private storage.
    func update(_ enabled: Bool) {
        let result = session.applyNetworkToggle(enabled)
        isOn = result.isOn
        message = result.message
    }
}
