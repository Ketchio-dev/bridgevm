import SwiftUI

struct LibraryDetailView: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if library.selectedID == LibraryModel.firstRunImportSelectionID {
            FirstRunView(library: library)
        } else if library.proMode {
            FleetTableView(library: library)
        } else if library.selectedID == LibraryModel.hvfEngineSelectionID {
            HvfEngineView()
        } else if let model = library.selectedModel {
            if model.config.engineKind == .hvfEngine, model.config.installPending == true {
                HvfWindowsInstallView(config: model.config, library: library)
                    .id(model.config.slug)
            } else if let hvfConfig = HvfEngineConfig.libraryVM(model.config) {
                HvfEngineView(config: hvfConfig)
                    .id(model.config.slug)
            } else {
                VMDetailPanel(model: model, library: library)
                    .id(model.config.slug)
            }
        } else if library.vms.isEmpty {
            FirstRunView(library: library)
        } else {
            emptyState
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Image(systemName: "desktopcomputer").font(.system(size: 48)).foregroundColor(.secondary)
            Text("VM을 선택하거나 새로 만드세요").foregroundColor(.secondary)
            Button { library.showingCreate = true } label: { Label("새 VM", systemImage: "plus") }
                .accessibilityIdentifier("bridgevm.library.empty.create")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
