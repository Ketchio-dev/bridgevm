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
                TextField("Windows에 입력할 텍스트", text: $draft, onCommit: submit)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.input")
                Button("입력", action: submit)
                    .disabled(draft.isEmpty || draftPending)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.send")
                    .help("클립보드로 입력하면 Windows에서 마지막으로 복사한 내용이 바뀝니다.")
                Button("Tab") { submissionRefused = !session.sendToolbarKey("tab") }
                Button("Enter") { submissionRefused = !session.sendToolbarKey("enter") }
                Button("Space") { submissionRefused = !session.sendToolbarKey("space") }
            }
            if draftPending {
                Label("Windows의 입력 확인을 기다리고 있습니다.", systemImage: "clock")
                    .font(.callout).foregroundStyle(.secondary)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.pending")
            } else if unconfirmedDraft != nil || submissionRefused {
                Label(unconfirmedDraft != nil
                      ? "입력 결과를 확인하지 못했습니다. 다시 보내기 전에 Windows에서 입력된 내용을 확인하세요."
                      : "입력을 접수하지 못했습니다. 작성한 내용은 유지됩니다. 연결 상태와 로그를 확인하세요.",
                      systemImage: "exclamationmark.circle")
                    .font(.callout).foregroundStyle(.orange)
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
