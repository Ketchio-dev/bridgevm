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
                    Text("Virtual machines").font(.body.weight(.medium))
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
            .accessibilityLabel("Virtual machines")
            .accessibilityValue(Self.vmCountText(library.vms.count))
            .accessibilityIdentifier("bridgevm.library.overview")
            .accessibilityAddTraits(library.selectedRetainedControl == nil ? .isSelected : [])
            .padding(.horizontal, 12).padding(.bottom, 8)
            LibraryRetainedControlsNavigation(library: library, didSelect: didShowRetained)
            VStack(spacing: 16) {
                LibraryHostSummary(library: library)
            }
            .padding(14)
        }
        .toolbar {
            ToolbarItem {
                Button { showOverview(in: presentation) } label: { Label("All VMs", systemImage: "square.grid.2x2") }
                    .help("Show all virtual machines")
            }
            ToolbarItem {
                Button { presentation?.wrappedValue.sheet = .create(installer: nil) } label: { Label("New VM", systemImage: "plus") }
                    .accessibilityIdentifier("bridgevm.library.toolbar.create")
            }
        }
    }

    /// VoiceOver reads the button's bare count badge with its unit.
    static func vmCountText(_ count: Int) -> String { count == 1 ? "1 VM" : "\(count) VMs" }

    func showOverview(in presentation: Binding<LibraryWindowPresentation>?) {
        if let presentation, presentation.wrappedValue.showsImportDraft {
            presentation.wrappedValue.showsImportDraft = false
            return
        }
        library.proMode = false
        // A failed import is finished: going back shows Get Started without the old error.
        if library.vms.isEmpty, library.selectedID == LibraryModel.firstRunImportSelectionID,
           library.discardFirstRunImportFailure() {
            library.selectedID = nil
        } else if library.selectedID == nil {
            library.selectedID = library.vms.first?.slug
        }
        didShowOverview?()
    }
}
