import SwiftUI

struct RetainedInstallControlView: View {
    let config: VMConfig
    @ObservedObject var session: HvfWindowsInstallSession
    let dismiss: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(config.name).font(.title2.bold())
                Text("등록 목록에서 사라졌을 때 보관한 기존 설치입니다.")
                    .foregroundColor(.secondary)
                Text(session.stage.label)
                if case let .failed(message) = session.stage {
                    Text(message).foregroundColor(.secondary)
                }
                if session.isRunning {
                    Button("취소") { session.cancel() }
                        .accessibilityIdentifier("bridgevm.retained.install.cancel")
                } else {
                    Button("목록에서 닫기", action: dismiss)
                        .accessibilityIdentifier("bridgevm.retained.install.dismiss")
                }
                Text(session.logLines.suffix(30).joined(separator: "\n"))
                    .font(.caption.monospaced())
            }
            .padding(20)
        }
    }
}
