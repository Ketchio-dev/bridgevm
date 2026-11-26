import SwiftUI

/// Restart for one VM. With the guest agent Windows restarts itself; otherwise the user confirms a
/// Reset, which restarts the VM at once, as after a power cut. The confirmation belongs to the guest
/// boot it was asked for: the session drops it when that boot ends, which closes the dialog.
@MainActor
struct HvfRestartControl<Content: View>: View {
    @ObservedObject var session: HvfEngineSession
    let identifier: String
    private let content: (_ enabled: Bool, _ press: @escaping () -> Void) -> Content
    /// This control asked; another control on the same session shows nothing.
    @State private var asked = false
    @State private var resultMessage: String?

    init(session: HvfEngineSession, identifier: String,
         @ViewBuilder content: @escaping (_ enabled: Bool, _ press: @escaping () -> Void) -> Content) {
        _session = ObservedObject(wrappedValue: session)
        self.identifier = identifier; self.content = content
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            content(session.vmmControlAvailable && !session.runtimeStartupWorkerPending) { press() }
            if let text = session.restartRequest?.kind.message ?? session.restartNotice ?? resultMessage {
                Text(text).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier(identifier + ".restart-status")
            }
        }
        .onChange(of: session.guestBoot) { _, _ in resultMessage = nil }
        .confirmationDialog(title, isPresented: confirming, titleVisibility: .visible,
                            presenting: session.resetConfirmation) { confirmation in
            if confirmation.prompt == .windowsHasNotRestarted {
                Button("Request Restart Again") { resultMessage = session.requestWindowsRestart().message }
                    .accessibilityIdentifier(identifier + ".restart-again")
            }
            Button("Reset", role: .destructive) { resultMessage = session.requestReset().message }
                .accessibilityIdentifier(identifier + ".confirm-reset")
            Button("Cancel", role: .cancel) { session.cancelResetConfirmation() }
        } message: { confirmation in
            Text(Self.message(confirmation.prompt))
        }
    }

    private var title: String { Self.title(session.resetConfirmation?.prompt ?? .agentUnavailable) }

    static func title(_ prompt: HvfResetPrompt) -> String {
        prompt == .windowsHasNotRestarted ? "Windows hasn’t restarted yet" : "Reset this VM?"
    }

    private var confirming: Binding<Bool> {
        Binding(get: { asked && session.resetConfirmation != nil },
                set: { if !$0 { asked = false } })
    }

    private func press() {
        resultMessage = nil
        let outcome = session.requestRestart()
        if case .needsResetConfirmation = outcome { asked = true } else { resultMessage = outcome.message }
    }

    static func message(_ prompt: HvfResetPrompt) -> String {
        let reset = "Resetting immediately power cycles the VM. You will lose any unsaved work."
        switch prompt {
        case .agentUnavailable:
            return "The Windows guest agent isn’t ready to handle a restart request. " + reset
        case .windowsHasNotRestarted:
            return "Check the VM display for apps blocking the restart. " + reset
        }
    }
}
