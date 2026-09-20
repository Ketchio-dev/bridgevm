import SwiftUI

struct RetainedRuntimeStopControl: View {
    @ObservedObject var session: HvfEngineSession
    @State private var message: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button("중지") {
                HvfRuntimeStopAction.perform(
                    session: session,
                    requestStop: { $0.stopOwned(expectedToken: $1.token) },
                    report: { message = $0 },
                    refresh: {})
            }
            .accessibilityIdentifier("bridgevm.retained.runtime.stop")
            if let message {
                Text(message).font(.caption).foregroundColor(.secondary)
            }
        }
    }
}
