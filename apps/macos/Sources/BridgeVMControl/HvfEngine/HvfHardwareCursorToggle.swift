import SwiftUI

/// The VM's "Use Mac pointer for Windows" setting (vm.json `hardwareCursor`). On, the default, the
/// app switches viogpudo's hardware cursor on and shows Windows' pointer as the Mac cursor
/// (HvfDisplayDriverHardwareCursor.swift, HvfGuestCursorPresenter.swift). Off, it writes HWCursor=0
/// instead and Windows draws its own pointer again. Shown once the VirtIO display driver is installed;
/// changed only while the VM is stopped, like the other per-VM settings.
struct HvfHardwareCursorToggle: View {
    @ObservedObject var session: HvfEngineSession
    @State private var changed = false
    @State private var message: String?

    static let note = "BridgeVM saves this in Windows on the next start. It takes effect after Windows restarts again."

    static func isShown(_ config: VMConfig?) -> Bool { config?.virtioDisplayDriverInstalled == true }

    var body: some View {
        if Self.isShown(session.config.libraryContext?.config) {
            VStack(alignment: .leading, spacing: 4) {
                Toggle("Use Mac pointer for Windows", isOn: Binding(get: { session.guestHardwareCursorWanted }, set: update))
                    .accessibilityIdentifier("bridgevm.runtime.hardware-cursor")
                    .disabled(!session.libraryPreferencesEditable)
                    .help("By default, macOS draws the Windows pointer so it follows your mouse immediately. Turn this off to let Windows draw the pointer. Changes made while the VM is stopped are saved for future starts.")
                if changed {
                    Text(Self.note).font(.caption).foregroundColor(.secondary)
                }
            }
            .alert("Pointer settings", isPresented: Binding(get: { message != nil }, set: { if !$0 { message = nil } })) {
                Button("OK") { message = nil }
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
        let message = saveLibraryPreference("Pointer", publish: publish) {
            $0.hardwareCursor = enabled ? nil : false
        }
        return (guestHardwareCursorWanted, message)
    }
}
