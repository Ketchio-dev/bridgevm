import SwiftUI

struct FleetTableView: View {
    @ObservedObject var library: LibraryModel
    var didShowVM: () -> Void = {}
    @State private var selection: String?
    @State private var order = [KeyPathComparator(\FleetVM.name, comparator: .localizedStandard)]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("VM configurations").font(.title2.bold())
                Spacer()
                Button("Done") { library.proMode = false }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()
            if library.vms.isEmpty {
                ContentUnavailableView("No virtual machines yet", systemImage: "desktopcomputer",
                                       description: Text("Create or import a VM to see its configuration."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                GeometryReader { size in
                    table(compact: size.size.width < 600)
                }
                HStack {
                    Text(library.vms.count == 1 ? "1 VM" : "\(library.vms.count) VMs")
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Show VM") { show(selection) }
                        .disabled(!library.vms.contains { $0.slug == selection })
                }
                .padding(.horizontal)
                .padding(.vertical, 10)
            }
        }
        .onAppear { selection = library.selectedID }
    }

    private func table(compact: Bool) -> some View {
        let sortOrder = order + [KeyPathComparator(\FleetVM.id, comparator: .localizedStandard)]
        let rows = library.vms.map(FleetVM.init).sorted(using: sortOrder)
        return Table(rows, selection: $selection, sortOrder: $order) {
            TableColumn("Virtual machine", value: \.name) { vm in
                VStack(alignment: .leading, spacing: 2) {
                    Text(vm.name).lineLimit(1)
                    Text(vm.id).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
                }
                .help("\(vm.name) · \(vm.id)")
            }
            .width(min: 115, ideal: compact ? 142 : 240)
            TableColumn("Engine", value: \.engine)
                .width(min: 64, ideal: compact ? 68 : 110)
            TableColumn("CPUs", value: \.cpus) { vm in
                Text(vm.config.cpuCount.map(String.init) ?? "Auto").monospacedDigit()
            }
            .width(min: 62, ideal: compact ? 64 : 78)
            TableColumn("Memory", value: \.memory) { vm in
                Text(vm.memoryLabel).monospacedDigit()
            }
            .width(min: 70, ideal: compact ? 76 : 105)
        }
        .alternatingRowBackgrounds(.disabled)
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first { Button("Show VM") { show(id) } }
        } primaryAction: { ids in show(ids.first) }
        .accessibilityLabel("VM configurations")
    }

    private func show(_ id: String?) {
        guard let id, library.vms.contains(where: { $0.slug == id }) else { return }
        didShowVM()
        library.selectedID = id
        library.proMode = false
    }
}

private struct FleetVM: Identifiable {
    let config: VMConfig
    var id: String { config.slug }
    var name: String { config.name }
    var engine: String { config.engineShortLabel }
    var cpus: Int { config.cpuCount ?? 0 }
    var memory: Int { config.memMiB ?? 0 }

    var memoryLabel: String {
        guard let mib = config.memMiB else { return "Auto" }
        return VMResourceLimits.memoryText(mib: mib)
    }
}
