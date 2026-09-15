import SwiftUI

struct LibrarySidebar: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        VStack(spacing: 0) {
            LibraryBrand()
            Button(action: showOverview) {
                HStack(spacing: 12) {
                    Image(systemName: "square.grid.2x2").font(.title3)
                    Text("모든 가상 머신").font(.body.weight(.medium))
                    Spacer(minLength: 0)
                    Text("\(library.vms.count)").font(.caption.monospacedDigit())
                }
                .padding(12)
                .foregroundStyle(library.proMode && library.selectedID == nil ? Color.blue : Color.primary)
                .background(library.proMode && library.selectedID == nil ? Color.blue.opacity(0.12) : Color.clear,
                            in: RoundedRectangle(cornerRadius: 10))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("bridgevm.library.overview")
            .accessibilityAddTraits(library.proMode && library.selectedID == nil ? .isSelected : [])
            .padding(.horizontal, 12).padding(.bottom, 8)
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
            VStack(spacing: 16) {
                LibraryHostSummary(library: library)
                LibraryEngineLegend()
            }
            .padding(14)
        }
        .toolbar {
            ToolbarItem {
                Button(action: showOverview) { Label("모든 VM", systemImage: "square.grid.2x2") }
                    .help("전체 VM 보기")
            }
            ToolbarItem {
                Button { library.showingCreate = true } label: { Label("새 VM", systemImage: "plus") }
                    .accessibilityIdentifier("bridgevm.library.toolbar.create")
            }
        }
    }

    private func showOverview() {
        library.selectedID = nil
        library.proMode = true
    }
}
