import SwiftUI

struct HvfWindowsSnapshotCard: View {
    let config: HvfEngineConfig
    @ObservedObject var session: HvfEngineSession
    @StateObject private var model = HvfWindowsSnapshotModel()
    @State private var name = ""

    var body: some View {
        let entries = HvfDiskGrow.bundle(config: config).map { HvfWindowsSnapshotStore.list(bundle: $0) } ?? []
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                if entries.isEmpty { Text("스냅샷 없음") }
                ForEach(entries) { entry in
                    HStack {
                        Text(entry.label)
                            .accessibilityIdentifier(identifier("date", entry, entries))
                        Spacer()
                        Button("복원") { request(.restore, entry) }
                            .accessibilityIdentifier(identifier("restore", entry, entries))
                        Button("삭제", role: .destructive) { request(.delete, entry) }
                            .accessibilityIdentifier(identifier("delete", entry, entries))
                    }
                    .disabled(session.hasActiveRuntimeWork || model.busy)
                }
                HStack(spacing: 8) {
                    TextField("이름 (선택 사항)", text: $name)
                        .accessibilityIdentifier("bridgevm.runtime.snapshot.name")
                    Button("새 스냅샷") {
                        let chosenName = name
                        model.request(.create, snapshotDate: nil) { [session, config] operation in
                            let notice = try await session.runSnapshot(operation, configuration: config, name: chosenName)
                            name = ""
                            return notice
                        }
                    }
                    .accessibilityIdentifier("bridgevm.runtime.snapshot.create")
                    if model.busy { ProgressView().controlSize(.small) }
                }
                .disabled(session.hasActiveRuntimeWork || model.busy)
                Text(model.status)
                    .font(.caption)
                    .foregroundColor(model.failed ? .red : .secondary)
                    .accessibilityIdentifier("bridgevm.runtime.snapshot.status")
            }
            .padding(6)
        } label: {
            Label("종료 상태 스냅샷", systemImage: "clock.arrow.circlepath")
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

    private func identifier(_ action: String, _ entry: HvfWindowsSnapshotStore.Entry,
                            _ entries: [HvfWindowsSnapshotStore.Entry]) -> String {
        "bridgevm.runtime.snapshot.\(action)" + (entry.id == entries.first?.id ? "" : ".\(entry.id)")
    }

    private func request(_ operation: HvfWindowsSnapshotCommand.Operation, _ entry: HvfWindowsSnapshotStore.Entry) {
        model.request(operation, snapshotDate: entry.date, label: entry.label) { [session, config] operation in
            try await session.runSnapshot(operation, configuration: config, target: entry.url)
        }
    }
}
