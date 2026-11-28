import SwiftUI

struct LibraryOperationAlerts: ViewModifier {
    @ObservedObject var library: LibraryModel

    func body(content: Content) -> some View {
        content.modifier(LibraryDeletionWarningAlert(library: library))
        .alert(
            "Couldn’t Delete VM",
            isPresented: Binding(
                get: { library.deletionError != nil },
                set: { if !$0 { library.deletionError = nil } }
            )
        ) {
            Button("OK") { library.deletionError = nil }
        } message: {
            Text(library.deletionError ?? "The operation could not be completed.")
        }
        .alert(
            "Couldn’t Duplicate VM",
            isPresented: Binding(
                get: { library.cloneError != nil },
                set: { if !$0 { library.cloneError = nil } }
            )
        ) {
            Button("OK") { library.cloneError = nil }
        } message: {
            Text(library.cloneError ?? "The operation could not be completed.")
        }
        .alert(
            "Couldn’t Move VM",
            isPresented: Binding(
                get: { library.moveError != nil },
                set: { if !$0 { library.moveError = nil } }
            )
        ) {
            Button("OK") { library.moveError = nil }
        } message: {
            Text(library.moveError ?? "The operation could not be completed.")
        }
        .alert("VM Settings Reloaded", isPresented: $library.showsSettingsReloadNotice) {
            Button("OK") { }
        } message: {
            Text("Your change was saved. This VM’s settings also changed on disk, so make any other unsaved changes again.")
        }
        .alert(
            "VM Action Unavailable",
            isPresented: Binding(
                get: { library.operationError != nil },
                set: { if !$0 { library.operationError = nil } }
            )
        ) {
            Button("OK") { library.operationError = nil }
        } message: {
            Text(library.operationError ?? "The operation could not be completed.")
        }
    }
}
