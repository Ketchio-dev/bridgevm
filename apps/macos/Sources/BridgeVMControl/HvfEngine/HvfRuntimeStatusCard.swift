import SwiftUI

struct HvfRuntimeStatusCard: View {
    @ObservedObject var session: HvfEngineSession
    let ready: Bool
    let stateText: String
    let heartbeatText: String
    let refusal: String?
    let start: () -> Void

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    Button(action: start) {
                        Label(session.runtimeStartupWorkerPending ? "준비 중…" : "시작", systemImage: "play.fill")
                    }
                    .buttonStyle(.borderedProminent).controlSize(.large)
                    .disabled(session.hasActiveRuntimeWork || !ready)
                    .accessibilityIdentifier("bridgevm.windows.runtime.start")
                    HvfRuntimeStatusStopControl(session: session)
                    if let pending = session.runtimePendingWorkText {
                        ProgressView().controlSize(.small).accessibilityLabel(pending)
                        Text(pending).font(.callout).foregroundStyle(.secondary)
                    }
                }
                if let failure = refusal ?? session.guiStartFailureText {
                    Text(failure).foregroundStyle(.red).textSelection(.enabled)
                        .accessibilityIdentifier("bridgevm.windows.runtime.start.failure")
                }
                HStack(spacing: 24) {
                    infoItem("State", stateText)
                    infoItem("Heartbeat", heartbeatText)
                    infoItem("Events", "\(session.events.count)")
                }
            }
            .padding(6)
        } label: {
            Label("실행 및 상태", systemImage: "play.circle")
        }
    }

    private func infoItem(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundColor(.secondary)
            Text(value).font(.body.monospaced())
        }
    }
}
