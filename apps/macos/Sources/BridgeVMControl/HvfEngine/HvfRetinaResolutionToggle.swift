import SwiftUI

/// Opt-in Retina resolution for the 2D virtio display (HvfDisplayResizeRequest). Only the app applies it,
/// but it is saved like the other per-VM preferences, which change only while the VM is fully stopped,
/// so it takes effect at the next start. Off again is saved as `false`, not removed: Windows keeps the
/// scale Retina mode set, so such a VM's requests keep naming 100 %.
struct HvfRetinaResolutionToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var setting: Bool?
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("Retina 해상도", isOn: Binding(get: { setting == true }, set: update))
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
        "실험적 기능입니다. 켜면 Retina 디스플레이에서 Windows가 Mac 화면의 픽셀 수 그대로 그리고 배율을 200%로 맞춥니다. CPU를 더 많이 사용하고, 일부 오래된 앱은 다시 열 때까지 흐리게 보일 수 있습니다. 아주 큰 창은 일반 해상도로 돌아갑니다. 한 번 켠 VM은 끈 뒤에도 창 크기가 바뀔 때마다 Windows 배율을 맞추므로(Retina로 그리지 않을 때는 100%), Windows에서 직접 바꾼 배율은 유지되지 않습니다."
            + (persists ? " VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : "")
    }

    /// On is `true`; off is `false` once it was set, and stays nil for a VM that never set it.
    static func saved(_ current: Bool?, enabled: Bool) -> Bool? {
        enabled ? true : current.map { _ in false }
    }

    private func update(_ enabled: Bool) {
        let result = session.applyRetinaResolutionToggle(enabled, current: setting)
        setting = result.setting
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the toggle holds afterwards: vm.json's value for a library VM, plus any message.
    func applyRetinaResolutionToggle(_ enabled: Bool, current: Bool? = nil,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (setting: Bool?, message: String?) {
        guard persistsLibraryPreferences else { return (HvfRetinaResolutionToggle.saved(current, enabled: enabled), nil) }
        let message = saveLibraryPreference("Retina 해상도 설정", publish: publish) {
            $0.retinaResolution = HvfRetinaResolutionToggle.saved($0.retinaResolution, enabled: enabled)
        }
        return (config.libraryContext?.config.retinaResolution, message)
    }
}
