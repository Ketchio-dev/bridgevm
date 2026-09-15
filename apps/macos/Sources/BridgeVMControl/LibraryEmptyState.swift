import SwiftUI

struct LibraryEmptyState: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        LibraryOverviewView(library: library, createIdentifier: "bridgevm.library.empty.create")
    }
}
