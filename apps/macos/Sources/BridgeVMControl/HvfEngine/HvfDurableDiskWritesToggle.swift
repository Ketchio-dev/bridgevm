import SwiftUI

/// Opt-in durable disk writes: every Windows flush waits for F_FULLFSYNC.
struct HvfDurableDiskWritesToggle: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var isOn: Bool
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Toggle("Durable disk writes", isOn: Binding(get: { isOn }, set: update))
            .accessibilityIdentifier("bridgevm.runtime.durable-disk-writes")
            .disabled(isDisabled)
            .help("끄면(기본) 디스크 쓰기 순서가 보장되어 Mac이 갑자기 꺼져도 Windows 디스크는 손상되지 않고, 마지막 1초 정도의 쓰기만 사라질 수 있습니다. 켜면 Windows가 확인한 쓰기는 하나도 사라지지 않지만, 내장 SSD에서 디스크 쓰기가 크게 느려집니다."
                + (session.persistsLibraryPreferences ? " VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : ""))
            .alert("디스크 쓰기 설정", isPresented: Binding(
                get: { message != nil }, set: { if !$0 { message = nil } }
            )) {
                Button("확인") { message = nil }
            } message: {
                Text(message ?? "")
            }
    }

    private func update(_ enabled: Bool) {
        let result = session.applyDurableDiskWritesToggle(enabled)
        isOn = result.isOn
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyDurableDiskWritesToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsLibraryPreferences else { return (enabled, nil) }
        let message = saveLibraryPreference("디스크 쓰기 설정", publish: publish) {
            $0.durableDiskWritesEnabled = enabled ? true : nil
        }
        return (config.libraryContext?.config.durableDiskWritesEnabled ?? false, message)
    }
}
