import SwiftUI

struct RetainedInstallControlView: View {
    let config: VMConfig
    @ObservedObject var session: HvfWindowsInstallSession
    let dismiss: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(config.name).font(.title2.bold())
                Text("This installation is still available here after its VM left the library.")
                    .foregroundColor(.secondary)
                Text(session.stage.label)
                if case let .failed(message) = session.stage {
                    Text(message).foregroundColor(.secondary)
                }
                if session.isRunning {
                    Button(session.stage == .cancelling ? "Cancelling…" : "Cancel") { session.cancel() }
                        .disabled(!session.canCancel).accessibilityIdentifier("bridgevm.retained.install.cancel")
                } else {
                    Button("Dismiss", action: dismiss)
                        .accessibilityIdentifier("bridgevm.retained.install.dismiss")
                }
                Text(session.logLines.suffix(30).joined(separator: "\n"))
                    .font(.caption.monospaced())
            }
            .padding(20)
        }
    }
}
