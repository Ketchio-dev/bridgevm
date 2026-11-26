import SwiftUI

struct LibrarySidebar: View {
    @Environment(\.libraryWindowPresentation) private var presentation
    @ObservedObject var library: LibraryModel
    var didShowOverview: (() -> Void)?
    var didShowRetained: (() -> Void)?

    var body: some View {
        VStack(spacing: 0) {
            LibraryBrand()
            Button { showOverview(in: presentation) } label: {
                HStack(spacing: 12) {
                    Image(systemName: "desktopcomputer").font(.title3)
                    Text("가상 머신").font(.body.weight(.medium))
                    Spacer(minLength: 0)
                    Text("\(library.vms.count)").font(.caption.monospacedDigit())
                }
                .padding(12)
                .foregroundStyle(Color.blue)
                .background(Color.blue.opacity(0.12),
                            in: RoundedRectangle(cornerRadius: 10))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("bridgevm.library.overview")
            .accessibilityAddTraits(.isSelected)
            .padding(.horizontal, 12).padding(.bottom, 8)
            LibraryRetainedControlsNavigation(library: library, didSelect: didShowRetained)
            VStack(spacing: 16) {
                LibraryHostSummary(library: library)
            }
            .padding(14)
        }
        .toolbar {
            ToolbarItem {
                Button { showOverview(in: presentation) } label: { Label("모든 VM", systemImage: "square.grid.2x2") }
                    .help("전체 VM 보기")
            }
            ToolbarItem {
                Button { presentation?.wrappedValue.sheet = .create(installer: nil) } label: { Label("새 VM", systemImage: "plus") }
                    .accessibilityIdentifier("bridgevm.library.toolbar.create")
            }
        }
    }

    func showOverview(in presentation: Binding<LibraryWindowPresentation>?) {
        if let presentation, presentation.wrappedValue.showsImportDraft {
            presentation.wrappedValue.showsImportDraft = false
            return
        }
        library.proMode = false
        if library.vms.isEmpty, library.selectedID == LibraryModel.firstRunImportSelectionID,
           !library.firstRunImportBusy, library.firstRunImportError == nil,
           library.firstRunImport.publishedConfig == nil {
            library.selectedID = nil
        } else if library.selectedID == nil {
            library.selectedID = library.vms.first?.slug
        }
        didShowOverview?()
    }
}
