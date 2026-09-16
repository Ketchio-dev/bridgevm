import SwiftUI

struct HvfWindowsInstallControls: View {
    @ObservedObject var session: HvfWindowsInstallSession

    var body: some View {
        HStack {
            Button(session.stage.primaryActionTitle) { _ = session.start() }
                .disabled(session.isRunning || session.stage == .done).accessibilityIdentifier("bridgevm.install.start")
            if session.isRunning {
                Button(session.stage == .cancelling ? "취소 중…" : "취소") { session.cancel() }
                    .disabled(!session.canCancel)
                    .accessibilityIdentifier("bridgevm.windows.install.cancel")
            }
            Spacer()
        }
    }
}
