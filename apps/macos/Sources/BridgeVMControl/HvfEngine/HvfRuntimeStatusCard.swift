import SwiftUI

struct HvfRuntimeStatusCard: View {
    @ObservedObject var session: HvfEngineSession
    let ready: Bool
    let stateText: String
    let stateIdentifier: String
    let refusal: String?
    let start: () -> Void

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    Button(action: start) {
                        Label(session.runtimeStartupWorkerPending ? "Preparing…" : "Start", systemImage: "play.fill")
                    }
                    .buttonStyle(.borderedProminent).controlSize(.large)
                    .disabled((session.hasActiveRuntimeWork && !session.runnerDiedWithoutCleanup) || session.runtimeStartupWorkerPending || !ready)
                    .accessibilityIdentifier("bridgevm.windows.runtime.start")
                    HvfRuntimeStatusStopControl(session: session)
                    HvfRestartControl(session: session, identifier: "bridgevm.windows.runtime") { enabled, press in
                        Button(action: press) { Label("Restart", systemImage: "arrow.counterclockwise") }
                            .disabled(!enabled).controlSize(.large)
                            .accessibilityIdentifier("bridgevm.windows.runtime.restart")
                    }
                    HvfPauseControl(session: session, identifier: "bridgevm.windows.runtime") { enabled, paused, press in
                        Button(action: press) {
                            Label(HvfPauseControl<EmptyView>.title(paused: paused),
                                  systemImage: HvfPauseControl<EmptyView>.symbol(paused: paused))
                        }
                        .disabled(!enabled).controlSize(.large)
                        .accessibilityIdentifier("bridgevm.windows.runtime.pause")
                    }
                    if let pending = session.runtimePendingWorkText {
                        ProgressView().controlSize(.small).accessibilityLabel(pending)
                        Text(pending).font(.callout).foregroundStyle(.secondary)
                    }
                }
                HvfRuntimeStatusMessages(session: session, refusal: refusal)
                HStack(spacing: 24) {
                    infoItem("State", stateText, stateIdentifier)
                    HvfRuntimeHeartbeatItem(heartbeat: session.heartbeat)
                    infoItem("Events", "\(session.events.count)", "bridgevm.windows.runtime.events")
                }
            }
            .padding(6)
        } label: {
            Label("Runtime status", systemImage: "play.circle")
        }
    }

    private func infoItem(_ label: String, _ value: String, _ identifier: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundColor(.secondary)
            Text(value).font(.body.monospaced()).accessibilityIdentifier(identifier)
        }
    }
}
