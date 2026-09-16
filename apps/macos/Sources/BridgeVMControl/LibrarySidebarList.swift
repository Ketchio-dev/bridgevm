import SwiftUI

struct LibrarySidebarList: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        List(selection: $library.selectedID) {
            FirstRunImportSidebarEntry(library: library)
            RetainedControlsSidebarEntry(library: library)
            Section("VM 라이브러리") {
                ForEach(library.vms) { config in
                    VMRow(config: config)
                        .tag(config.slug)
                        .contextMenu { LibraryVMContextMenu(library: library, config: config) }
                }
            }
            if !library.libraryIssues.isEmpty {
                LibraryIssuesSection(issues: library.libraryIssues)
            }
            Section("실험") {
                Label("HVF Engine", systemImage: "cpu")
                    .tag(LibraryModel.hvfEngineSelectionID)
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .onChange(of: library.selectedID) { _, selection in
            if selection != nil { library.proMode = false }
        }
    }
}
