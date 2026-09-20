import SwiftUI

struct HvfRuntimeStatusStopControl: View {
    @ObservedObject var session: HvfEngineSession
    @State private var message: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button {
                HvfRuntimeStopAction.perform(session: session,
                    requestStop: { $0.stopOwned(expectedToken: $1.token) },
                    report: { message = $0 }, refresh: {})
            } label: { Label("중지", systemImage: "stop.fill") }
            .disabled(!presentation.isEnabled).controlSize(.large)
            .help(presentation.guidance).accessibilityHint(presentation.guidance)
            .accessibilityIdentifier("bridgevm.windows.runtime.stop")
            if let message { Text(message).font(.caption).foregroundStyle(.secondary) }
        }
    }

    private var presentation: HvfRuntimeStopPresentation {
        .make(runtimeActive: session.connectionState != .stopped || session.mayHaveOwnedWork,
              lifecycleBusy: session.runtimeStartupWorkerPending || session.connectionState == .stopping,
              ownsRuntime: session.ownedProcessIdentity != nil)
    }
}
