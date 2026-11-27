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

/// Host-only settings (how this app maps keys or sizes the display) change while Windows runs. A running
/// VM keeps vm.json as it was admitted with (HvfLibraryPreferenceSave), so the session uses a change at once
/// and it is saved once the VM stops, as HvfDisplayDriverFlag saves its flag.
@MainActor
enum HvfHostSettingLiveSave {
    typealias Change = (inout VMConfig) -> Void
    private(set) static var pending: [String: [String: Change]] = [:]

    static func record(_ setting: String, for slug: String, _ change: @escaping Change) {
        pending[slug, default: [:]][setting] = change
    }

    /// The slug to record a change for, or nil when the setting saves now (stopped) or must wait (a start
    /// or another change is being prepared, which disables the control).
    static func liveSlug(_ session: HvfEngineSession) -> String? {
        guard !session.libraryPreferencesEditable, !session.hasPendingRuntimeStart,
              !session.hasPendingRuntimeMutation else { return nil }
        return session.config.libraryContext?.config.slug
    }

    /// vm.json's configuration with this VM's changes that wait for the stop, for controls that show them.
    static func preview(_ config: VMConfig) -> VMConfig {
        guard let changes = pending[config.slug] else { return config }
        var copy = config
        for (_, change) in changes.sorted(by: { $0.key < $1.key }) { change(&copy) }
        return copy
    }

    @discardableResult
    static func saveIfStopped(_ session: HvfEngineSession,
                              publish: @escaping HvfEngineSession.RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard let slug = session.config.libraryContext?.config.slug, let changes = pending[slug],
              session.libraryPreferencesEditable else { return nil }
        var failure: String?
        for (setting, change) in changes.sorted(by: { $0.key < $1.key }) {
            if let message = session.saveLibraryPreference(setting, publish: publish, change: change) {
                failure = message
            } else {
                pending[slug]?[setting] = nil
            }
        }
        if pending[slug]?.isEmpty == true { pending[slug] = nil }
        return failure
    }
}

extension HvfEngineSession {
    /// What the picker shows afterwards: vm.json's value for a library VM (the chosen mode while it runs),
    /// plus any message.
    func applyCommandKeyMode(_ mode: HvfCommandKeyMode,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (mode: HvfCommandKeyMode, message: String?) {
        guard persistsLibraryPreferences else { return (mode, nil) }
        let change: HvfHostSettingLiveSave.Change = { $0.commandKeyMode = mode == .mac ? nil : mode.rawValue }
        if let slug = HvfHostSettingLiveSave.liveSlug(self) {
            HvfHostSettingLiveSave.record("Command key", for: slug, change)
            return (mode, nil) // the picker's binding puts it in `config`, which keyMapping reads
        }
        let message = saveLibraryPreference("Command key", publish: publish, change: change)
        return (HvfCommandKeyMode(saved: config.libraryContext?.config.commandKeyMode), message)
    }
}
