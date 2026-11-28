import SwiftUI

struct RetainedRuntimeControlView: View {
    let config: VMConfig
    @ObservedObject var session: HvfEngineSession
    let dismiss: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(config.name).font(.title2.bold())
                Text("This VM is no longer in your library, but BridgeVM kept its controls here.")
                    .foregroundColor(.secondary)
                Text(LibraryRetainedControlDescriptor.runtime(config: config, session: session).statusLabel)
                RetainedRuntimeHeartbeatText(heartbeat: session.heartbeat)
                if session.connectionState != .stopped {
                    RetainedRuntimeStopControl(session: session)
                } else {
                    Button("Dismiss", action: dismiss)
                        .accessibilityIdentifier("bridgevm.retained.runtime.dismiss")
                }
                // The same bounded event lines as the dashboard's Guest agent events, not Swift's debug form.
                Text(HvfEventFeedText.render(session.events))
                    .font(.caption.monospaced())
            }
            .padding(20)
        }
    }
}
