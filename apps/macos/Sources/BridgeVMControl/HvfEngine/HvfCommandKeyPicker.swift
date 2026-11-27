import SwiftUI

/// The per-VM 'Command key' setting (HvfCommandKeyMode).
struct HvfCommandKeyPicker: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var selection: HvfCommandKeyMode
    @State private var message: String?

    /// Host-only: the mode changes how this app maps keys, nothing in Windows, so it is not locked while
    /// Windows runs, only while a start or another change is being prepared.
    var isDisabled: Bool {
        session.persistsLibraryPreferences && (session.hasPendingRuntimeStart || session.hasPendingRuntimeMutation)
    }

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
            + (session.persistsLibraryPreferences ? " Changes apply at once and are saved for future starts." : ""))
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

/// A running VM keeps vm.json as it was admitted with (HvfLibraryPreferenceSave), so a mode chosen while it
/// runs applies to the session at once and is saved once it stops, as HvfDisplayDriverFlag saves its flag.
@MainActor
enum HvfCommandKeyLiveSave {
    private(set) static var pending: [String: HvfCommandKeyMode] = [:]

    static func record(_ mode: HvfCommandKeyMode, for slug: String) { pending[slug] = mode }

    @discardableResult
    static func saveIfStopped(_ session: HvfEngineSession,
                              publish: @escaping HvfEngineSession.RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard let slug = session.config.libraryContext?.config.slug, let mode = pending[slug],
              session.libraryPreferencesEditable else { return nil }
        let message = session.saveCommandKeyMode(mode, publish: publish)
        if message == nil { pending[slug] = nil }
        return message
    }
}

extension HvfEngineSession {
    /// What the picker shows afterwards: vm.json's value for a library VM (the chosen mode while it runs),
    /// plus any message.
    func applyCommandKeyMode(_ mode: HvfCommandKeyMode,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (mode: HvfCommandKeyMode, message: String?) {
        guard persistsLibraryPreferences else { return (mode, nil) }
        if !libraryPreferencesEditable, !hasPendingRuntimeStart, !hasPendingRuntimeMutation,
           let slug = config.libraryContext?.config.slug {
            HvfCommandKeyLiveSave.record(mode, for: slug)
            return (mode, nil) // the picker's binding puts it in `config`, which keyMapping reads
        }
        let message = saveCommandKeyMode(mode, publish: publish)
        return (HvfCommandKeyMode(saved: config.libraryContext?.config.commandKeyMode), message)
    }

    func saveCommandKeyMode(_ mode: HvfCommandKeyMode, publish: RegistrationPublish) -> String? {
        saveLibraryPreference("Command key", publish: publish) {
            $0.commandKeyMode = mode == .mac ? nil : mode.rawValue
        }
    }
}
