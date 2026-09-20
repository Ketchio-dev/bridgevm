import SwiftUI

struct HvfWindowsInstallStatusCard: View {
    @ObservedObject var session: HvfWindowsInstallSession

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("진행 상태").font(.headline)
            HStack(spacing: 8) {
                if session.isRunning { ProgressView().controlSize(.small) }
                Text(session.stage.label)
                    .foregroundColor(stageColor)
                    .accessibilityIdentifier("bridgevm.windows.install.stage")
            }
            if case let .failed(message) = session.stage {
                Text(message).font(.caption).foregroundColor(.red)
                    .accessibilityIdentifier("bridgevm.windows.install.failure")
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled)
            }
            if let startedAt = session.startedAt, session.isRunning {
                Text("경과: \(startedAt, style: .timer)")
                    .font(.caption).foregroundColor(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Color.gray.opacity(0.08))
        .cornerRadius(10)
    }

    private var stageColor: Color {
        switch session.stage {
        case .done: return .green
        case .failed: return .red
        case .cancelled: return .secondary
        default: return .primary
        }
    }
}
