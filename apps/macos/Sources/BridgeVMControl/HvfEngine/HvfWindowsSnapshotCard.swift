import SwiftUI

struct HvfWindowsSnapshotCard: View {
    let config: HvfEngineConfig
    @ObservedObject var session: HvfEngineSession
    @StateObject private var model = HvfWindowsSnapshotModel()

    var body: some View {
        let date = HvfWindowsSnapshotCommand.snapshotDate(config: config)
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text(date.map { "마지막 스냅샷: " + HvfWindowsSnapshotModel.dateText($0) } ?? "스냅샷 없음")
                    .accessibilityIdentifier("bridgevm.runtime.snapshot.date")
                HStack(spacing: 8) {
                    Button("스냅샷 생성") { model.request(.create, snapshotDate: date, run: run) }
                        .accessibilityIdentifier("bridgevm.runtime.snapshot.create")
                    Button("스냅샷 복원") { model.request(.restore, snapshotDate: date, run: run) }
                        .disabled(date == nil)
                        .accessibilityIdentifier("bridgevm.runtime.snapshot.restore")
                    if model.busy { ProgressView().controlSize(.small) }
                    Spacer()
                }
                .disabled(session.hasActiveRuntimeWork || model.busy)
                Text(model.status)
                    .font(.caption)
                    .foregroundColor(model.failed ? .red : .secondary)
                    .accessibilityIdentifier("bridgevm.runtime.snapshot.status")
            }
            .padding(6)
        } label: {
            Label("Powered-off Snapshot", systemImage: "clock.arrow.circlepath")
        }
        .confirmationDialog(model.confirmation?.title ?? "", isPresented: Binding(
            get: { model.confirmation != nil }, set: { if !$0 { model.cancel() } }
        ), titleVisibility: .visible, presenting: model.confirmation) { confirmation in
            Button(confirmation.action, role: .destructive) { model.confirm(confirmation) }
                .accessibilityIdentifier("bridgevm.runtime.snapshot.confirm")
            Button("취소", role: .cancel) { model.cancel() }
        } message: { confirmation in
            Text(confirmation.message)
        }
    }

    /// The configuration the user asked about, captured when they asked.
    private var run: HvfWindowsSnapshotModel.Run {
        { [session, config] operation in try await session.runSnapshot(operation, configuration: config) }
    }
}
