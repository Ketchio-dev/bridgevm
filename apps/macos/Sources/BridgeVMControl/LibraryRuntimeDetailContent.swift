import SwiftUI

struct LibraryRuntimeDetailContent: View {
    let config: VMConfig
    @ObservedObject var library: LibraryModel

    @ViewBuilder var body: some View {
        if config.engineKind == .hvfEngine, let session = library.hvfRuntimeDetailSession(for: config) {
            HvfEngineView(session: session).id(ObjectIdentifier(session))
        } else {
            LibraryUnavailableRuntimeDetail(config: config, library: library)
        }
    }
}

struct LibraryUnavailableRuntimeDetail: View {
    let config: VMConfig
    @ObservedObject var library: LibraryModel

    var body: some View {
        ContentUnavailableView {
            Label("VM controls unavailable", systemImage: "exclamationmark.shield")
        } description: {
            Text("The saved VM configuration has changed. Wait for the current operation to finish, then reload the library.")
        } actions: {
            Button("Reload Library", action: library.reload)
        }
        .navigationTitle(config.name)
        .accessibilityIdentifier("bridgevm.runtime.unavailable")
    }
}
