import SwiftUI

/// Restart for one VM. With the guest agent Windows restarts itself; otherwise the user confirms a
/// Reset, which restarts the VM at once, as after a power cut.
@MainActor
struct HvfRestartControl<Content: View>: View {
    @ObservedObject var session: HvfEngineSession
    let identifier: String
    private let content: (_ enabled: Bool, _ press: @escaping () -> Void) -> Content
    @State private var prompt: HvfResetPrompt?
    @State private var promptedRun: HvfOwnedRuntimeIdentity?
    @State private var resultMessage: String?

    init(session: HvfEngineSession, identifier: String,
         @ViewBuilder content: @escaping (_ enabled: Bool, _ press: @escaping () -> Void) -> Content) {
        _session = ObservedObject(wrappedValue: session)
        self.identifier = identifier; self.content = content
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            content(session.vmmControlAvailable && !session.runtimeStartupWorkerPending) { press() }
            if let text = session.restartRequest?.message ?? resultMessage {
                Text(text).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier(identifier + ".restart-status")
            }
        }
        .onChange(of: session.restartRequest) { _, _ in resultMessage = nil }
        .confirmationDialog(title, isPresented: confirming, titleVisibility: .visible, presenting: prompt) { prompt in
            if prompt == .windowsHasNotRestarted {
                Button("다시 요청") { finish(session.requestWindowsRestart()) }
                    .accessibilityIdentifier(identifier + ".restart-again")
            }
            Button("재설정", role: .destructive) { finish(session.requestReset()) }
                .accessibilityIdentifier(identifier + ".confirm-reset")
            Button("취소", role: .cancel) { self.prompt = nil }
        } message: { prompt in
            Text(Self.message(prompt))
        }
    }

    private var title: String {
        prompt == .windowsHasNotRestarted ? "Windows가 아직 다시 시작하지 않았습니다" : "VM을 재설정할까요?"
    }

    private var confirming: Binding<Bool> {
        Binding(get: { prompt != nil }, set: { if !$0 { prompt = nil } })
    }

    private func press() {
        resultMessage = nil
        switch session.requestRestart() {
        case let .needsResetConfirmation(next):
            promptedRun = session.ownedProcessIdentity
            prompt = next
        case let outcome:
            resultMessage = outcome.message
        }
    }

    /// The answer applies only to the run it was asked for.
    private func finish(_ request: @autoclosure () -> HvfRestartOutcome) {
        defer { prompt = nil }
        guard session.ownedProcessIdentity == promptedRun else {
            resultMessage = "VM 실행이 바뀌어 요청을 보내지 않았습니다."
            return
        }
        resultMessage = request().message
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
