import SwiftUI

struct LibrarySidebarList: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        List(selection: $library.selectedID) {
            FirstRunImportSidebarEntry(library: library)
            RetainedControlsSidebarEntry(library: library)
            Section {
                ForEach(library.vms) { config in
                    VMRow(config: config)
                        .tag(config.slug)
                        .contextMenu { LibraryVMContextMenu(library: library, config: config) }
                }
            } header: {
                Text("VM 라이브러리")
                    .foregroundStyle(Color.secondary)
            }
            if !library.libraryIssues.isEmpty {
                LibraryIssuesSection(issues: library.libraryIssues)
            }
            Section {
                Label("HVF Engine", systemImage: "cpu")
                    .foregroundStyle(Color.primary)
                    .tag(LibraryModel.hvfEngineSelectionID)
            } header: {
                Text("실험")
                    .foregroundStyle(Color.secondary)
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .onChange(of: library.selectedID) { _, selection in
            if selection != nil { library.proMode = false }
        }
    }
}
