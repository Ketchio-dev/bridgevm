import SwiftUI
import AppKit

struct LibraryVMContextMenu: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig

    var body: some View {
        if config.engineKind == .hvfEngine, config.installPending != true {
            Button { library.requestWindowsClone(config) } label: {
                Label("새 TPM ID로 복제", systemImage: "plus.square.on.square")
            }
            .disabled(library.cloningSlugs.contains(config.slug))
            Button { chooseMoveDestination() } label: {
                Label("같은 VM ID로 번들 이동", systemImage: "folder.badge.gearshape")
            }
            .disabled(library.movingSlugs.contains(config.slug))
        }
        Button(role: .destructive) { library.requestDeletion(config) } label: {
            Label("삭제", systemImage: "trash")
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
        panel.prompt = "여기로 이동"
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        library.moveWindowsHVFBundle(config, to: destination)
    }
}
