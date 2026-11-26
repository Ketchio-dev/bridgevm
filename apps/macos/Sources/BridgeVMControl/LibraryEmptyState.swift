import SwiftUI

struct LibraryEmptyState: View {
    var body: some View {
        ContentUnavailableView("Select a virtual machine", systemImage: "desktopcomputer",
            description: Text("Choose a VM from the list to see its status and settings."))
    }
}
