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

    var statusText: String {
        if let status = model.status { return status }
        return session.hasActiveRuntimeWork
            ? "To create or restore a snapshot, shut down Windows and wait for other VM operations to finish."
            : "Create a snapshot to save the Windows disk and boot settings."
    }

    var body: some View {
        let listRequest = ListRequest(bundle: HvfDiskGrow.bundle(config: config), refreshID: model.listRevision)
        let entries = listedEntries ?? []
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                // In the grouped settings form a titled field turns into a row label with a borderless
                // input; a hidden label with a prompt and a border reads as something to type into.
                TextField("Snapshot name", text: $name, prompt: Text("Snapshot name (optional)"))
                    .labelsHidden()
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.runtime.snapshot.name")
                Button("Create Snapshot") {
                    let chosenName = name
                    model.create { [session, config] operation in
                        let notice = try await session.runSnapshot(operation, configuration: config, name: chosenName)
                        name = ""
                        return notice
                    }
                }
                .accessibilityIdentifier("bridgevm.runtime.snapshot.create")
                // The status line below says what is under way.
                if model.busy { ProgressView().controlSize(.small).accessibilityHidden(true) }
            }
            .disabled(session.hasActiveRuntimeWork || model.busy)
            Text(statusText)
                .font(.caption)
                .foregroundColor(model.failed ? .red : .secondary)
                .accessibilityIdentifier("bridgevm.runtime.snapshot.status")
            if listedEntries == nil {
                ProgressView("Loading snapshots…").controlSize(.small)
            } else if entries.isEmpty {
                Text("No snapshots yet").font(.callout).foregroundStyle(.secondary)
            }
            if !entries.isEmpty { Divider() }
            ForEach(entries) { entry in
                let label = entry.label
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
                    Button("Restore") { request(.restore, entry) }
                        .accessibilityLabel("Restore \(label)")
                        .accessibilityIdentifier(identifier("restore", entry, entries))
                        .disabled(session.hasActiveRuntimeWork)
                    // Deleting frees space while Windows runs (HvfWindowsSnapshotExecution.deleteSnapshot).
                    Button("Delete", role: .destructive) { request(.delete, entry) }
                        .accessibilityLabel("Delete \(label)")
                        .accessibilityIdentifier(identifier("delete", entry, entries))
                }
                .disabled(model.busy)
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
            Button("Cancel", role: .cancel) {}
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
