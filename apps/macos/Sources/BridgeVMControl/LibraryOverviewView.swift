import SwiftUI

struct LibraryOverviewView: View {
    @ObservedObject var library: LibraryModel

    @Binding var query: String

    var body: some View {
        LibraryWorkspaceList(library: library, query: $query)
    }
}
