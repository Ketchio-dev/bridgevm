import SwiftUI

/// The per-VM 'Command key' setting (HvfCommandKeyMode).
struct HvfCommandKeyPicker: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var selection: HvfCommandKeyMode
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }

    var body: some View {
        Picker("Command key", selection: Binding(get: { selection }, set: update)) {
            Text("Mac shortcuts").tag(HvfCommandKeyMode.mac)
            Text("Windows key").tag(HvfCommandKeyMode.windows)
        }
        .fixedSize()
        .accessibilityIdentifier("bridgevm.runtime.command-key")
        .disabled(isDisabled)
        .help("In Mac shortcuts mode (the default), Windows receives Command shortcuts such as ⌘C and ⌘V as Ctrl shortcuts. Use ⌘←/→ for line boundaries, ⌘↑/↓ for document boundaries, ⌥←/→ to move by word, and ⌥⌫ to delete a word. Hold Shift with an arrow shortcut to select text. Press and release the left Command key alone to open Start. The right Command key remains available for input switching. "
            + "In Windows key mode, Command acts as the Windows key. For example, ⌘E opens File Explorer. In both modes, other Option combinations send Alt. macOS keeps ⌘Q, ⌘H, ⌘M, ⌘Comma, ⌃⌘F, and ⌘Tab."
            + (session.persistsLibraryPreferences ? " Changes made while the VM is stopped are saved for future starts." : ""))
        .alert("Command key settings", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("OK") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    private func update(_ mode: HvfCommandKeyMode) {
        let result = session.applyCommandKeyMode(mode)
        selection = result.mode
        message = result.message
    }
}

extension HvfEngineSession {
    /// What the picker shows afterwards: vm.json's value for a library VM, plus any message.
    func applyCommandKeyMode(_ mode: HvfCommandKeyMode,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (mode: HvfCommandKeyMode, message: String?) {
        guard persistsLibraryPreferences else { return (mode, nil) }
        let message = saveLibraryPreference("Command key", publish: publish) {
            $0.commandKeyMode = mode == .mac ? nil : mode.rawValue
        }
        return (HvfCommandKeyMode(saved: config.libraryContext?.config.commandKeyMode), message)
    }
}
