import SwiftUI

/// Shared-folder controls of the runtime view. For a library VM, turning sharing off, choosing a
/// folder, Return in either field, and runtime-view Start save the visible pair in vm.json.
struct HvfSharedFolderSettings: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var enabled: Bool
    @Binding var hostDir: String
    @Binding var guestDir: String
    @State private var message: String?
    var commit: (() -> Void)? = nil
    @FocusState private var focusedField: Field?
    private enum Field { case host, guest }

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }
    /// The visible pair, or nil while either half is still empty.
    var draft: HvfSharedFolderSelection? {
        hostDir.isEmpty || guestDir.isEmpty ? nil : HvfSharedFolderSelection(hostPath: hostDir, guestPath: guestDir)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Toggle("Share a folder", isOn: Binding(get: { enabled }, set: setEnabled))
                .accessibilityIdentifier("bridgevm.runtime.share.enabled")
                .help(session.persistsLibraryPreferences ? "The folder you choose while the VM is stopped is saved for future starts." : "")
            if enabled {
                HStack {
                    Text("Mac folder").frame(width: 92, alignment: .leading)
                    TextField("Mac folder", text: $hostDir)
                        .textFieldStyle(.roundedBorder)
                        .font(.body.monospaced())
                        .accessibilityIdentifier("bridgevm.runtime.share.host")
                        .focused($focusedField, equals: .host)
                        .onSubmit(commitDraft)
                    Button(action: chooseHost) { Image(systemName: "folder") }
                        .accessibilityIdentifier("bridgevm.runtime.share.host.choose")
                        .accessibilityLabel("Choose Mac folder")
                        .help("Choose a Mac folder to share")
                }
                HStack {
                    Text("Windows folder").frame(width: 92, alignment: .leading)
                    TextField("C:\\bridgevm-share", text: $guestDir)
                        .textFieldStyle(.roundedBorder)
                        .font(.body.monospaced()).accessibilityIdentifier("bridgevm.runtime.share.guest")
                        .focused($focusedField, equals: .guest)
                        .onSubmit(commitDraft)
                }
            }
        }
        .disabled(isDisabled)
        .onChange(of: focusedField) { old, _ in if old != nil { commitDraft() } }
        .alert("Shared folder settings", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("OK") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    /// Turning sharing on only reveals the fields; nothing is shared until a folder is chosen.
    func setEnabled(_ on: Bool) {
        enabled = on
        if let commit { commit() }
        else if !on { apply(nil) } else if let draft { apply(draft) }
    }

    func commitDraft() {
        if let commit { commit(); return }
        if enabled, let draft { apply(draft) }
    }

    private func chooseHost() {
        #if canImport(AppKit)
        FileSelection.choose(directories: true) { url in
            hostDir = url.path
            commitDraft()
        }
        #endif
    }

    /// A saved or refused stop shows vm.json's pair; a refused selection stays a visible draft.
    private func apply(_ requested: HvfSharedFolderSelection?) {
        let result = session.applySharedFolderSelection(requested)
        message = result.message
        if requested != nil, result.selection != requested { return }
        enabled = result.selection != nil
        if let saved = result.selection {
            hostDir = saved.hostPath
            guestDir = saved.guestPath
        }
    }
}
