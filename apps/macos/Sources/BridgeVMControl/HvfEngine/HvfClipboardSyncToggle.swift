import SwiftUI

struct HvfClipboardSyncToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    /// It changes while Windows runs too (HvfEngineSession.applyClipboardSyncToggle), not during a start or
    /// another change.
    var isDisabled: Bool {
        session.persistsLibraryPreferences && !session.libraryPreferencesEditable && HvfHostSettingLiveSave.liveSlug(session) == nil
    }

    var body: some View {
        Toggle("Share clipboard", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.clipboard")
            .disabled(isDisabled)
            .help(session.persistsLibraryPreferences ? "Saved for future starts. While Windows runs, it takes effect at once." : "")
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
