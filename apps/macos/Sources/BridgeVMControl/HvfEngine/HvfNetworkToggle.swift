import SwiftUI

struct HvfNetworkToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("네트워크 사용", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.network")
            .disabled(isDisabled)
            .help(session.persistsLibraryPreferences ? "VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : "")
            .alert("네트워크 설정", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("확인") { message = nil }
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
