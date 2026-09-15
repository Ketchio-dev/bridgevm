import SwiftUI

struct HvfWindowsInstallControls: View {
    @ObservedObject var session: HvfWindowsInstallSession

    var body: some View {
        HStack {
            Button(session.isRunning ? "설치 진행 중…" : "설치 시작") { _ = session.start() }
                .disabled(session.isRunning || session.stage == .done).accessibilityIdentifier("bridgevm.install.start")
            if session.isRunning {
                Button(session.stage == .cancelling ? "취소 중…" : "취소") { session.cancel() }
                    .disabled(session.stage == .cancelling)
                    .accessibilityIdentifier("bridgevm.windows.install.cancel")
            }
            Spacer()
        }
    }
}
