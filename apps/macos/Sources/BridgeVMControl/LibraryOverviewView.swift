import SwiftUI

struct LibraryOverviewView: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        LibraryWorkspaceList(library: library)
    }
}
