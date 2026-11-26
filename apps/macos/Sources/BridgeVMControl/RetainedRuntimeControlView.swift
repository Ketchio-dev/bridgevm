import SwiftUI

struct RetainedRuntimeControlView: View {
    let config: VMConfig
    @ObservedObject var session: HvfEngineSession
    let dismiss: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(config.name).font(.title2.bold())
                Text("This session is still available here after the VM left your library.")
                    .foregroundColor(.secondary)
                Text(LibraryRetainedControlDescriptor.runtime(config: config, session: session).statusLabel)
                RetainedRuntimeHeartbeatText(heartbeat: session.heartbeat)
                if session.connectionState != .stopped {
                    RetainedRuntimeStopControl(session: session)
                } else {
                    Button("Dismiss", action: dismiss)
                        .accessibilityIdentifier("bridgevm.retained.runtime.dismiss")
                }
                Text(session.events.suffix(30).map { String(describing: $0) }.joined(separator: "\n"))
                    .font(.caption.monospaced())
            }
            .padding(20)
        }
    }
}
