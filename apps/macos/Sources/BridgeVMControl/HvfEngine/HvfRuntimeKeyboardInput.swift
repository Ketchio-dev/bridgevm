import SwiftUI

struct HvfRuntimeKeyboardInput: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var draft: String
    @State private var submissionRefused = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                TextField("Windows에 입력할 텍스트", text: $draft, onCommit: submit)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.input")
                Button("입력", action: submit)
                    .disabled(draft.isEmpty)
                    .accessibilityIdentifier("bridgevm.runtime.keyboard.send")
                    .help("클립보드로 입력하면 Windows에서 마지막으로 복사한 내용이 바뀝니다.")
                Button("Tab") { submissionRefused = !session.sendToolbarKey("tab") }
                Button("Enter") { submissionRefused = !session.sendToolbarKey("enter") }
                Button("Space") { submissionRefused = !session.sendToolbarKey("space") }
            }
            if submissionRefused {
                Label("입력을 접수하지 못했습니다. 작성한 내용은 유지됩니다. 연결 상태와 로그를 확인하세요.",
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
        let submitted = draft
        let editor = $draft
        submissionRefused = HvfKeyboardDraft.submit(&draft, to: session) {
            if editor.wrappedValue.utf8.elementsEqual(submitted.utf8) { editor.wrappedValue = "" }
        } == .refused
    }
}
