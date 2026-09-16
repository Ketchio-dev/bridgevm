import SwiftUI

// Preserve the existing overview route identity used by the native routing tests.
struct FleetTableView: View {
    @ObservedObject var library: LibraryModel
    var body: some View { LibraryWorkspaceList(library: library) }
}

struct LibraryOverviewView: View {
    @ObservedObject var library: LibraryModel
    var createIdentifier = "bridgevm.library.overview.create"

    var body: some View {
        LibraryWorkspaceList(library: library, createIdentifier: createIdentifier)
    }
}
