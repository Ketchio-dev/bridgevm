import SwiftUI

struct RetainedRuntimeControlView: View {
    let config: VMConfig
    @ObservedObject var session: HvfEngineSession
    let dismiss: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(config.name).font(.title2.bold())
                Text("등록 목록에서 사라졌을 때 보관한 기존 실행입니다.")
                    .foregroundColor(.secondary)
                Text(LibraryRetainedControlDescriptor.runtime(config: config, session: session).statusLabel)
                if let age = session.lastHeartbeatAge {
                    Text("최근 응답: \(age, specifier: "%.1f")초 전").font(.caption)
                }
                if session.connectionState != .stopped {
                    Button("중지") { session.stop() }
                        .accessibilityIdentifier("bridgevm.retained.runtime.stop")
                } else {
                    Button("목록에서 닫기", action: dismiss)
                        .accessibilityIdentifier("bridgevm.retained.runtime.dismiss")
                }
                Text(session.events.suffix(30).map { String(describing: $0) }.joined(separator: "\n"))
                    .font(.caption.monospaced())
            }
            .padding(20)
        }
    }
}
