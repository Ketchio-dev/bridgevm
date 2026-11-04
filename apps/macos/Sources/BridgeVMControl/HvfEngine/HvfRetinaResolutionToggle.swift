import SwiftUI

/// Opt-in Retina resolution for the 2D virtio display (HvfDisplayResizeRequest). Only the app applies it,
/// but it is saved like the other per-VM preferences, which change only while the VM is fully stopped,
/// so it takes effect at the next start.
struct HvfRetinaResolutionToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("Retina resolution", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.retina-resolution")
            .disabled(isDisabled)
            .help(Self.helpText(persists: session.persistsLibraryPreferences))
            .alert("Retina 해상도 설정", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("확인") { message = nil }
            } message: {
                Text(message ?? "")
            }
    }

    static func helpText(persists: Bool) -> String {
        "실험적 기능입니다. 켜면 Retina 디스플레이에서 Windows가 Mac 화면의 픽셀 수 그대로 그리고 배율을 200%로 맞춥니다. CPU를 더 많이 사용하고, 일부 오래된 앱은 다시 열 때까지 흐리게 보일 수 있습니다. 아주 큰 창은 일반 해상도로 돌아갑니다."
            + (persists ? " VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : "")
    }

    private func update(_ enabled: Bool) {
        let result = session.applyRetinaResolutionToggle(enabled)
        isOn = result.isOn
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyRetinaResolutionToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsLibraryPreferences else { return (enabled, nil) }
        let message = saveLibraryPreference("Retina 해상도 설정", publish: publish) {
            $0.retinaResolution = enabled ? true : nil
        }
        return (config.libraryContext?.config.retinaResolution ?? false, message)
    }
}
