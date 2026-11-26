import SwiftUI
struct LibraryDetailView: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        // Keep the same view and its draft fields while switching between the
        // two empty-library entry routes.
        if library.selectedID == LibraryModel.firstRunImportSelectionID
            || (library.selectedID == nil && library.vms.isEmpty && !library.proMode) {
            FirstRunView(library: library)
        } else if let record = library.selectedRetainedControl {
            RetainedControlDetailView(library: library, record: record)
        } else if library.proMode {
            FleetTableView(library: library)
        } else if library.selectedID == LibraryModel.hvfEngineSelectionID {
            HvfEngineView(session: library.experimentalHvfRuntimeSession())
        } else if let detail = library.selectedDetail {
            if library.shouldShowWindowsInstall(for: detail.config) {
                HvfWindowsInstallPreparationView(config: detail.config, library: library)
                    .id(detail.config.slug)
            } else if let session = library.hvfRuntimeDetailSession(for: detail.config) {
                HvfEngineView(session: session).id(ObjectIdentifier(session))
            } else if detail.config.engineKind == .hvfEngine {
                LibraryUnavailableRuntimeDetail(config: detail.config, library: library)
            } else {
                VMDetailPanel(model: detail.model, library: library).id(detail.config.slug)
            }
        } else if library.vms.isEmpty {
            FirstRunView(library: library)
        } else {
            LibraryEmptyState()
        }
    }
}
