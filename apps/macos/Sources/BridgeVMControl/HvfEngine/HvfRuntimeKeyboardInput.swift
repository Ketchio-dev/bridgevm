import SwiftUI

struct HvfRuntimeKeyboardInput: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var draft: String
    @State private var submissionRefused = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                TextField("Type text into Windows", text: $draft, onCommit: submit)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.input")
                Button("Type", action: submit)
                    .disabled(draft.isEmpty)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.send")
                Button("Tab") { session.sendKey("tab") }
                Button("Enter") { session.sendKey("enter") }
                Button("Space") { session.sendKey("space") }
            }
            if submissionRefused {
                Label("입력을 접수하지 못했습니다. 작성한 내용은 유지됩니다. 연결 상태와 로그를 확인한 뒤 다시 시도하세요.",
                      systemImage: "exclamationmark.circle")
                    .font(.callout).foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.refusal")
            }
        }
        .onChange(of: draft) { _, _ in submissionRefused = false }
    }

    private func submit() {
        guard !draft.isEmpty else { return }
        submissionRefused = HvfKeyboardDraft.submit(&draft, to: session) == .refused
    }
}
