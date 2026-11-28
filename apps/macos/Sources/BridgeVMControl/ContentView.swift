import SwiftUI
#if canImport(AppKit)
import AppKit
#endif
struct ContentView: View {
    @ObservedObject var library: LibraryModel
    @State private var presentation = LibraryWindowPresentation()
    @ObservedObject private var requests = LibraryWindowRequests.shared
    @State private var libraryQuery = ""
    @State private var showingCompactList = false
    @State private var importDraft = FirstRunFormDraft()
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    private var allowsVMComparison: Bool {
        library.selectedID != LibraryModel.firstRunImportSelectionID && library.selectedRetainedControl == nil
    }

    var body: some View {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        let _ = AppUIHost.prepared?.lifecycle.record(.contentBodyEvaluated)
        #endif
        GeometryReader { geometry in
            if library.proMode && allowsVMComparison {
                FleetTableView(library: library) { showingCompactList = false }
            } else if geometry.size.width < 850 {
                CompactLibraryNavigation(library: library, query: $libraryQuery, showingList: $showingCompactList)
            } else {
                let wide = geometry.size.width >= 1100
                libraryNavigation
                    .onChange(of: wide, initial: true) { _, wide in
                        columnVisibility = wide ? .all : .doubleColumn
                    }
            }
        }
        .environment(\.firstRunFormDraft, $importDraft)
        .onChange(of: library.selectedID) { _, selection in
            if selection != nil { showingCompactList = false }
            if let selection, selection != LibraryModel.firstRunImportSelectionID {
                importDraft = FirstRunFormDraft()
            }
        }
        .tint(LibraryAppearance.accent)
        .groupBoxStyle(LibraryGroupBoxStyle())
        .background(LibraryAppearance.canvas)
        .environment(\.libraryWindowPresentation, $presentation)
        .onChange(of: requests.createRequested, initial: true) { requests.take(into: &presentation) }
        .sheet(item: $presentation.sheet) { sheet in
            switch sheet {
            case .palette:
                CommandPaletteView(library: library) { presentation.sheet = .create(installer: nil) }
            case .create(let installer):
                CreateVMSheet(library: library, isoPath: installer?.path ?? "")
            }
        }
        .sheet(item: $presentation.cloneSource) { cfg in
            CloneWindowsHVFSheet(library: library, config: cfg)
        }
        .confirmationDialog(
            presentation.deletionTarget.map { LibraryDeletionPrompt(config: $0, rootURL: library.rootURL).title } ?? "",
            isPresented: Binding(
                get: { presentation.deletionTarget != nil },
                set: { if !$0 { presentation.deletionTarget = nil } }
            ),
            titleVisibility: .visible,
            presenting: presentation.deletionTarget
        ) { config in
            Button(LibraryDeletionPrompt(config: config, rootURL: library.rootURL).confirmTitle, role: .destructive) {
                library.confirmDeletion(config)
            }
            Button("Cancel", role: .cancel) { }
        } message: { config in
            Text(LibraryDeletionPrompt(config: config, rootURL: library.rootURL).message)
        }
        .modifier(LibraryOperationAlerts(library: library))
        .toolbar {
            if allowsVMComparison && !library.proMode && library.vms.count > 1 {
                ToolbarItem(placement: .primaryAction) {
                    Button { library.proMode = true } label: {
                        Label("Compare VMs", systemImage: "tablecells")
                    }
                    .help("Compare VM configurations")
                    .accessibilityIdentifier("bridgevm.library.toolbar.compare")
                }
            }
            ToolbarItem(placement: .primaryAction) {
                Button { presentation.sheet = .palette } label: {
                    Label("Command Palette", systemImage: "magnifyingglass")
                }
                .keyboardShortcut("k", modifiers: .command)
                .help("Search commands or VMs (⌘K)")
                .accessibilityIdentifier("bridgevm.library.toolbar.palette")
            }
        }
    }

    @ViewBuilder
    private var libraryNavigation: some View {
        if library.vms.isEmpty && library.libraryIssues.isEmpty {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                sidebar
            } detail: {
                LibraryDashboardRoute(library: library)
            }
        } else {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                sidebar
            } content: {
                LibraryOverviewView(library: library, query: $libraryQuery)
                    .navigationSplitViewColumnWidth(min: 390, ideal: 480, max: 580)
            } detail: {
                LibraryDashboardRoute(library: library)
            }
        }
    }

    private var sidebar: some View {
        LibrarySidebar(library: library)
            .navigationSplitViewColumnWidth(min: 220, ideal: 240, max: 270)
    }
}
