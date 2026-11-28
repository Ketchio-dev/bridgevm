#if canImport(AppKit)
import SwiftUI

/// File > New VM… and Window > Library reach the one library window, also after its close
/// button or with a VM's display in front. New VM… replaces SwiftUI's File > New Window,
/// which opened a second library window.
struct ControlLibraryCommands: Commands {
    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New VM…") { Self.newVM() }
                .keyboardShortcut("n", modifiers: .command)
        }
        CommandGroup(before: .windowArrangement) {
            Button("Library") { ControlLibraryWindow.show() }
            Divider()
        }
    }

    @MainActor static func newVM() {
        LibraryWindowRequests.shared.createRequested = true
        ControlLibraryWindow.show()
    }
}
#endif
