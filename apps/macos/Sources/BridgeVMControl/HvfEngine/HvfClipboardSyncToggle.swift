import SwiftUI

struct HvfClipboardSyncToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsClipboardSyncPreference && !session.clipboardSyncPreferenceEditable }

    var body: some View {
        Toggle("Clipboard sync", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.clipboard")
            .disabled(isDisabled)
            .help(session.persistsClipboardSyncPreference ? "VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : "")
            .alert("클립보드 동기화 설정", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("확인") { message = nil }
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
