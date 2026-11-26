import SwiftUI

struct HvfRuntimeKeyboardInput: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var draft: String
    @State private var submissionRefused = false
    @State private var unconfirmedDraft: UUID?

    private var draftPending: Bool { session.submittedKeyboardDraft != nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                TextField("Text to send to Windows", text: $draft, onCommit: submit)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.input")
                Button("Send", action: submit)
                    .disabled(draft.isEmpty || draftPending)
                    .accessibilityLabel("Send text to Windows")
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.send")
                    .help("When BridgeVM uses paste to send text, it replaces the contents of the Windows clipboard.")
            }
            if draftPending {
                Label("Waiting for Windows to confirm input…", systemImage: "clock")
                    .font(.callout).foregroundStyle(.secondary)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.pending")
            } else if unconfirmedDraft != nil || submissionRefused {
                Label {
                    Text(unconfirmedDraft != nil
                         ? "Check what appeared in Windows before sending this text again."
                         : "Couldn’t send the text. Check the VM connection and try again. Your draft is still here.")
                } icon: {
                    Image(systemName: "exclamationmark.circle").foregroundStyle(.orange)
                }
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier(unconfirmedDraft != nil ? "bridgevm.runtime.keyboard.unconfirmed"
                                             : "bridgevm.runtime.keyboard.refusal")
            }
        }
        .onChange(of: draft) { _, _ in submissionRefused = false }
    }

    private func submit() {
        guard !draft.isEmpty, !draftPending else { return }
        let submitted = draft
        let editor = $draft
        let confirmation = $unconfirmedDraft
        let previousAttempt = unconfirmedDraft
        // Refresh the pending feedback even when the user resends the same text.
        let attempt = UUID()
        unconfirmedDraft = attempt
        let result = HvfKeyboardDraft.submit(&draft, to: session) {
            guard confirmation.wrappedValue == attempt else { return }
            if editor.wrappedValue.utf8.elementsEqual(submitted.utf8) { editor.wrappedValue = "" }
            confirmation.wrappedValue = nil
        }
        submissionRefused = result == .refused
        if result != .acceptedForProcessing {
            unconfirmedDraft = submissionRefused ? previousAttempt : nil
        }
    }
}
