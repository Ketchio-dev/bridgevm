import SwiftUI

/// Small windows show one destination at a time so the dashboard's controls fit beside their labels.
struct CompactLibraryNavigation: View {
    @ObservedObject var library: LibraryModel
    @State private var showingList = false
    @State private var showingSidebar = false

    var body: some View {
        let hasLibrary = !library.vms.isEmpty || !library.libraryIssues.isEmpty || !library.retainedControlRecords.isEmpty
        let showsOverview = hasLibrary && (showingList || library.selectedID == nil)
        VStack(spacing: 0) {
            HStack {
                if hasLibrary && (!showsOverview || library.selectedID != nil) {
                    Button { showingList = !showsOverview } label: {
                        Label(showsOverview ? "상세 보기" : "가상 머신 목록",
                              systemImage: showsOverview ? "chevron.forward" : "chevron.backward")
                    }
                    .keyboardShortcut(showsOverview ? "]" : "[", modifiers: .command)
                    .accessibilityIdentifier(showsOverview ? "bridgevm.library.compact.detail" : "bridgevm.library.compact.list")
                }
                Spacer()
                Button { showingSidebar.toggle() } label: {
                    Image(systemName: "sidebar.leading")
                }
                .accessibilityLabel("사이드바")
                .popover(isPresented: $showingSidebar) {
                    LibrarySidebar(library: library, didShowOverview: {
                        showingSidebar = false
                        showingList = true
                    }, didShowRetained: {
                        showingSidebar = false
                        showingList = false
                    })
                    .frame(width: 250, height: 380)
                }
            }
            .padding(.horizontal, 18).padding(.vertical, 8)
            Divider()
            if showsOverview {
                LibraryWorkspaceList(library: library, focusesSearchOnAppear: true, didSelect: { showingList = false })
            } else {
                LibraryDashboardRoute(library: library)
                    .background {
                        if !library.vms.isEmpty {
                            Button("VM 검색") { showingList = true }
                                .keyboardShortcut("f", modifiers: .command)
                                .focusable(false).accessibilityHidden(true).opacity(0)
                        }
                    }
            }
        }
        .onChange(of: library.selectedID) { _, id in
            if id != nil {
                showingList = false
                showingSidebar = false
            }
        }
    }
}
