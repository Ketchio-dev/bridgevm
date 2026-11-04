import SwiftUI

/// The VM's "Windows 커서를 Mac 커서로 표시" setting (vm.json `hardwareCursor`). On, the default, the
/// app switches viogpudo's hardware cursor on and shows Windows' pointer as the Mac cursor
/// (HvfDisplayDriverHardwareCursor.swift, HvfGuestCursorPresenter.swift). Off, it writes HWCursor=0
/// instead and Windows draws its own pointer again. Shown once the VirtIO display driver is installed;
/// changed only while the VM is stopped, like the other per-VM settings.
struct HvfHardwareCursorToggle: View {
    @ObservedObject var session: HvfEngineSession
    @State private var changed = false
    @State private var message: String?

    static let note = "다음 VM 시작 때 Windows에 기록되고, 그다음 Windows 시작부터 적용됩니다."

    static func isShown(_ config: VMConfig?) -> Bool { config?.virtioDisplayDriverInstalled == true }

    var body: some View {
        if Self.isShown(session.config.libraryContext?.config) {
            VStack(alignment: .leading, spacing: 4) {
                Toggle("Windows 커서를 Mac 커서로 표시", isOn: Binding(get: { session.guestHardwareCursorWanted }, set: update))
                    .accessibilityIdentifier("bridgevm.runtime.hardware-cursor")
                    .disabled(!session.libraryPreferencesEditable)
                    .help("켜면(기본) Windows 포인터를 Mac 커서로 그려 마우스를 지연 없이 따라갑니다. 끄면 Windows가 포인터를 직접 그립니다. VM이 중지된 동안 변경하면 이 VM에 저장됩니다.")
                if changed {
                    Text(Self.note).font(.caption).foregroundColor(.secondary)
                }
            }
            .alert("커서 설정", isPresented: Binding(get: { message != nil }, set: { if !$0 { message = nil } })) {
                Button("확인") { message = nil }
            } message: {
                Text(message ?? "")
            }
        }
    }

    private func update(_ enabled: Bool) {
        let result = session.applyHardwareCursorToggle(enabled)
        if result.message == nil, result.isOn == enabled { changed = true }
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the toggle shows afterwards, vm.json's value, plus any refusal. Off is saved as false; on
    /// removes the key, the registration of a VM that never changed it.
    func applyHardwareCursorToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        let message = saveLibraryPreference("커서 설정", publish: publish) {
            $0.hardwareCursor = enabled ? nil : false
        }
        return (guestHardwareCursorWanted, message)
    }
}
