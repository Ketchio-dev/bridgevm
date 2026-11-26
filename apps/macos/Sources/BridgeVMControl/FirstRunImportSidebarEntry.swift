import SwiftUI

struct FirstRunImportSidebarEntry: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if library.firstRunImportBusy || library.firstRunImportError != nil
            || library.firstRunImport.publishedConfig != nil
            || library.selectedID == LibraryModel.firstRunImportSelectionID {
            Section("Import") {
                Label("Import VM", systemImage: "square.and.arrow.down")
                    .tag(LibraryModel.firstRunImportSelectionID)
                    .accessibilityIdentifier("bridgevm.library.import.status")
                    .help("View progress or continue importing.")
            }
        }
    }
}
