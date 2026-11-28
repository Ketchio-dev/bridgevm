import SwiftUI

struct HvfWindowsInstallControls: View {
    @ObservedObject var session: HvfWindowsInstallSession

    var body: some View {
        if session.isRunning {
            Button(session.stage == .cancelling ? "Canceling…" : "Cancel") { session.cancel() }
                .disabled(!session.canCancel)
                .accessibilityIdentifier("bridgevm.windows.install.cancel")
        } else if session.stage != .done {
            Button(session.stage.primaryActionTitle) { _ = session.start() }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("bridgevm.install.start")
        }
    }
}
