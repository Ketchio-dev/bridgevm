import SwiftUI
import AppKit

struct LibraryVMContextMenu: View {
    @Environment(\.libraryWindowPresentation) private var presentation
    @ObservedObject var library: LibraryModel
    let config: VMConfig

    var body: some View {
        Toggle("Start when BridgeVM opens", isOn: Binding(
            get: { library.latestConfiguration(for: config).startWithBridgeVM == true },
            set: { library.setStartWithBridgeVM($0, for: config) }))
            .disabled(!library.canSetStartWithBridgeVM(for: config))
            .help("Hold Option while opening BridgeVM to skip automatic startup.")
        if config.engineKind == .hvfEngine, let session = library.hvfRuntimeSessions.existingRecord(slug: config.slug)?.session {
            HvfPauseMenuItem(session: session)
        }
        if config.engineKind == .hvfEngine, config.installPending != true {
            Button { presentation?.wrappedValue.cloneSource = library.requestWindowsClone(config) } label: {
                Label("Duplicate with a new TPM identity…", systemImage: "plus.square.on.square")
            }
            .disabled(library.cloningSlugs.contains(config.slug))
            Button { chooseMoveDestination() } label: {
                Label("Move bundle, keeping VM identity…", systemImage: "folder.badge.gearshape")
            }
            .disabled(library.movingSlugs.contains(config.slug))
        }
        Button(role: .destructive) { presentation?.wrappedValue.deletionTarget = library.requestDeletion(config) } label: {
            Label("Delete…", systemImage: "trash")
        }
        .disabled(library.deletingSlugs.contains(config.slug)
                  || library.cloningSlugs.contains(config.slug)
                  || library.movingSlugs.contains(config.slug))
    }

    private func chooseMoveDestination() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Move Here"
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        library.moveWindowsHVFBundle(config, to: destination)
    }
}
