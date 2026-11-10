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
                Button("다시 요청") { resultMessage = session.requestWindowsRestart().message }
                    .accessibilityIdentifier(identifier + ".restart-again")
            }
            Button("재설정", role: .destructive) { resultMessage = session.requestReset().message }
                .accessibilityIdentifier(identifier + ".confirm-reset")
            Button("취소", role: .cancel) { session.cancelResetConfirmation() }
        } message: { confirmation in
            Text(Self.message(confirmation.prompt))
        }
    }

    private var title: String { Self.title(session.resetConfirmation?.prompt ?? .agentUnavailable) }

    static func title(_ prompt: HvfResetPrompt) -> String {
        prompt == .windowsHasNotRestarted ? "Windows가 아직 다시 시작하지 않았습니다" : "VM을 재설정할까요?"
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
        let reset = "재설정하면 전원을 껐다 켠 것처럼 VM이 바로 다시 시작되고, 저장하지 않은 작업은 사라집니다."
        switch prompt {
        case .agentUnavailable:
            return "Windows 에이전트가 준비되지 않아 Windows에 다시 시작을 요청할 수 없습니다. " + reset
        case .windowsHasNotRestarted:
            return "앱이 다시 시작을 막고 있으면 VM 화면에서 확인하세요. " + reset
        }
    }
}
