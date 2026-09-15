import SwiftUI

struct LibraryDetailView: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if library.selectedID == LibraryModel.firstRunImportSelectionID {
            FirstRunView(library: library)
        } else if library.proMode {
            FleetTableView(library: library)
        } else if library.selectedID == LibraryModel.hvfEngineSelectionID {
            HvfEngineView(session: library.experimentalHvfRuntimeSession())
        } else if let detail = library.selectedDetail {
            if library.shouldShowWindowsInstall(for: detail.config) {
                HvfWindowsInstallView(config: detail.config, library: library)
                    .id(detail.config.slug)
            } else if let session = library.hvfRuntimeSession(for: detail.config) {
                HvfEngineView(session: session)
                    .id(ObjectIdentifier(session))
            } else {
                VMDetailPanel(model: detail.model, library: library)
                    .id(detail.config.slug)
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
