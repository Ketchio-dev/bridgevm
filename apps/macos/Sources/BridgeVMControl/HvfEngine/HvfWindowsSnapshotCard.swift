import SwiftUI

struct HvfWindowsSnapshotCard: View {
    let config: HvfEngineConfig
    @ObservedObject var session: HvfEngineSession
    var listEntries: @Sendable (URL) -> [HvfWindowsSnapshotStore.Entry] = { HvfWindowsSnapshotStore.list(bundle: $0) }
    @Environment(\.controlActiveState) private var controlActiveState
    @State private var listedEntries: [HvfWindowsSnapshotStore.Entry]?
    @ObservedObject private var model: HvfWindowsSnapshotModel
    @State private var confirmation: HvfWindowsSnapshotModel.Confirmation?
    @State private var name = ""

    init(config: HvfEngineConfig, session: HvfEngineSession) {
        self.config = config
        self.session = session
        model = session.snapshotModel
    }

    private struct ListRequest: Equatable {
        let bundle: URL?
        let refreshID: Int
    }

    var body: some View {
        let listRequest = ListRequest(bundle: HvfDiskGrow.bundle(config: config), refreshID: model.listRevision)
        let entries = listedEntries ?? []
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                // In the grouped settings form a titled field turns into a row label with a borderless
                // input; a hidden label with a prompt and a border reads as something to type into.
                TextField("스냅샷 이름", text: $name, prompt: Text("스냅샷 이름 (선택 사항)"))
                    .labelsHidden()
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.runtime.snapshot.name")
                Button("새 스냅샷") {
                    let chosenName = name
                    model.create { [session, config] operation in
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
            if listedEntries == nil {
                ProgressView("스냅샷 불러오는 중…").controlSize(.small)
            } else if entries.isEmpty {
                Text("스냅샷 없음").font(.callout).foregroundStyle(.secondary)
            }
            if !entries.isEmpty { Divider() }
            ForEach(entries) { entry in
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        if let name = entry.name {
                            Text(name)
                                .fontWeight(.medium)
                                .lineLimit(2)
                                .help(name)
                        }
                        Text(HvfWindowsSnapshotModel.dateText(entry.date))
                            .font(entry.name == nil ? .body : .caption)
                            .foregroundStyle(entry.name == nil ? .primary : .secondary)
                            .monospacedDigit()
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier(identifier("date", entry, entries))
                    Spacer()
                    Button("복원") { request(.restore, entry) }
                        .accessibilityIdentifier(identifier("restore", entry, entries))
                    Button("삭제", role: .destructive) { request(.delete, entry) }
                        .accessibilityIdentifier(identifier("delete", entry, entries))
                }
                .disabled(session.hasActiveRuntimeWork || model.busy)
            }
        }
        .padding(.vertical, 4)
        .task(id: listRequest) {
            let entries = await Task.detached(priority: .userInitiated) { [listEntries] in
                listRequest.bundle.map(listEntries) ?? []
            }.value
            if !Task.isCancelled { listedEntries = entries }
        }
        .onChange(of: controlActiveState) { _, state in
            if state == .key { model.requestListRefresh() }
        }
        .confirmationDialog(confirmation?.title ?? "", isPresented: Binding(
            get: { confirmation != nil }, set: { if !$0 { confirmation = nil } }
        ), titleVisibility: .visible, presenting: confirmation) { confirmation in
            Button(confirmation.action, role: .destructive) { model.confirm(confirmation) }
                .accessibilityIdentifier("bridgevm.runtime.snapshot.confirm")
            Button("취소", role: .cancel) {}
        } message: { confirmation in
            Text(confirmation.message)
        }
    }

    private func identifier(_ action: String, _ entry: HvfWindowsSnapshotStore.Entry,
                            _ entries: [HvfWindowsSnapshotStore.Entry]) -> String {
        "bridgevm.runtime.snapshot.\(action)" + (entry.id == entries.first?.id ? "" : ".\(entry.id)")
    }

    private func request(_ operation: HvfWindowsSnapshotCommand.Operation, _ entry: HvfWindowsSnapshotStore.Entry) {
        confirmation = model.confirmation(for: operation, snapshotDate: entry.date, label: entry.label) { [session, config] operation in
            try await session.runSnapshot(operation, configuration: config, target: entry.url)
        }
    }
}
