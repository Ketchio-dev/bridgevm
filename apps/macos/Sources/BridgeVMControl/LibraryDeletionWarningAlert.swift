import SwiftUI

/// A completed deletion whose vTPM key could not be archived. The VM is gone, so
/// this is never presented as a deletion failure.
struct LibraryDeletionWarningAlert: ViewModifier {
    @ObservedObject var library: LibraryModel

    func body(content: Content) -> some View {
        content.alert(
            "VM Deleted",
            isPresented: Binding(
                get: { library.deletionWarning != nil },
                set: { if !$0 { library.deletionWarning = nil } }
            )
        ) {
            Button("OK") { library.deletionWarning = nil }
        } message: {
            Text(library.deletionWarning ?? "")
        }
    }
}
