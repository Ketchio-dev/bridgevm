import SwiftUI

struct LibraryStartupView: View {
    @ObservedObject var startup: LibraryStartupCoordinator
    var body: some View {
        Group {
            if let model = startup.model {
                ContentView(library: model)
            } else {
                VStack(spacing: 16) {
                    if let failure = startup.failure {
                        Image(systemName: "exclamationmark.triangle").font(.largeTitle)
                        Text("Couldn’t open the VM library").font(.title2)
                        Text(failure).textSelection(.enabled)
                        Button("Try Again") { Task { await startup.load() } }
                    } else {
                        ProgressView()
                        Text("Opening your VM library").font(.title2)
                        Text("Checking for installations to recover before showing your VMs.")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(40)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task { await startup.load() }
    }
}
