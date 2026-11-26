import SwiftUI

struct LibraryRetainedControlsNavigation: View {
    @ObservedObject var library: LibraryModel
    var didSelect: (() -> Void)?

    var selection: Binding<String?> {
        Binding(get: { library.selectedID }, set: { token in
            guard let token, library.retainedControlRecords.contains(where: { $0.id == token }) else { return }
            library.proMode = false
            library.selectedID = token
            didSelect?()
        })
    }

    var body: some View {
        if library.retainedControlRecords.isEmpty {
            Spacer(minLength: 12)
        } else {
            List(selection: selection) {
                RetainedControlsSidebarEntry(library: library)
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .accessibilityIdentifier("bridgevm.library.retained-controls")
        }
    }
}
