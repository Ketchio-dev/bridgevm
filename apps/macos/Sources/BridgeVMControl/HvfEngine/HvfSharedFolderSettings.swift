import SwiftUI

/// Shared-folder controls of the runtime view. For a library VM, turning sharing off, choosing a
/// folder, Return in either field, and runtime-view Start save the visible pair in vm.json.
struct HvfSharedFolderSettings: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var enabled: Bool
    @Binding var hostDir: String
    @Binding var guestDir: String
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }
    /// The visible pair, or nil while either half is still empty.
    var draft: HvfSharedFolderSelection? {
        hostDir.isEmpty || guestDir.isEmpty ? nil : HvfSharedFolderSelection(hostPath: hostDir, guestPath: guestDir)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Toggle("Shared folder", isOn: Binding(get: { enabled }, set: setEnabled))
                .accessibilityIdentifier("bridgevm.runtime.share.enabled")
                .help(session.persistsLibraryPreferences ? "VM이 중지된 동안 선택한 폴더는 이 VM의 모든 시작에 저장됩니다." : "")
            if enabled {
                HStack {
                    Text("Host share").frame(width: 92, alignment: .leading)
                    TextField("Host share", text: $hostDir)
                        .textFieldStyle(.roundedBorder)
                        .font(.body.monospaced())
                        .accessibilityIdentifier("bridgevm.runtime.share.host")
                        .onSubmit(commitDraft)
                    Button(action: chooseHost) { Image(systemName: "folder") }
                        .accessibilityIdentifier("bridgevm.runtime.share.host.choose")
                        .help("Choose Host share")
                }
                HStack {
                    Text("Guest share").frame(width: 92, alignment: .leading)
                    TextField("C:\\bridgevm-share", text: $guestDir)
                        .textFieldStyle(.roundedBorder)
                        .font(.body.monospaced()).accessibilityIdentifier("bridgevm.runtime.share.guest")
                        .onSubmit(commitDraft)
                }
            }
        }
        .disabled(isDisabled)
        .alert("공유 폴더 설정", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("확인") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    /// Turning sharing on only reveals the fields; nothing is shared until a folder is chosen.
    func setEnabled(_ on: Bool) {
        enabled = on
        if !on { apply(nil) } else if let draft { apply(draft) }
    }

    func commitDraft() {
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
