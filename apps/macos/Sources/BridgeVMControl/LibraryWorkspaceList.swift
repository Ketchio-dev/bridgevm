import SwiftUI

struct LibraryWorkspaceList: View {
    @Environment(\.libraryWindowPresentation) private var presentation
    @ObservedObject var library: LibraryModel
    @Binding var query: String
    var focusesSearchOnAppear = false
    var didSelect: (() -> Void)?

    private var matchingVMs: [VMConfig] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty else { return library.vms }
        return library.vms.filter {
            $0.name.localizedStandardContains(term)
                || $0.displayName.localizedStandardContains(term)
                || $0.engineShortLabel.localizedStandardContains(term)
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if library.vms.isEmpty {
                emptyState
            } else {
                LibrarySearchField(query: $query, focusesOnAppear: focusesSearchOnAppear)
                    .padding(.horizontal, 22).padding(.vertical, 16)
                let results = matchingVMs
                ScrollView {
                    LazyVStack(spacing: 12) {
                        ForEach(results) { config in
                            LibraryWorkspaceRow(
                                library: library,
                                config: config,
                                model: library.model(for: config),
                                selected: library.selectedID == config.slug && !library.proMode,
                                didSelect: didSelect
                            )
                        }
                    }
                    .padding(.horizontal, 22).padding(.bottom, 22)
                }
                .overlay {
                    if results.isEmpty {
                        ContentUnavailableView {
                            Label("No results", systemImage: "magnifyingglass")
                        } description: {
                            Text("Try a different VM name or engine.")
                        }
                        .accessibilityIdentifier("bridgevm.library.search.no-results")
                    }
                }
            }
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle("Virtual machines")
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .center, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Virtual machines").font(.system(size: 28, weight: .bold))
                    Text(library.vms.count == 1 ? "1 VM" : "\(library.vms.count) VMs")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Button {
                    library.proMode = false
                    library.selectedID = LibraryModel.firstRunImportSelectionID
                    didSelect?()
                } label: {
                    Image(systemName: "square.and.arrow.down")
                }
                .controlSize(.large)
                .help("Import an existing VM")
                .accessibilityLabel("Import VM")
                .accessibilityIdentifier("bridgevm.library.overview.import")
                Button { presentation?.wrappedValue.sheet = .create(installer: nil) } label: {
                    Label("New VM", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent).controlSize(.large)
                .accessibilityIdentifier("bridgevm.library.overview.create")
            }
            if !library.libraryIssues.isEmpty {
                LibraryIssuesSection(issues: library.libraryIssues, compact: true)
            }
        }
        .padding(22)
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No virtual machines yet", systemImage: "desktopcomputer")
        } description: {
            Text("Create a VM or import an existing one.")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct LibraryWorkspaceRow: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig
    @ObservedObject var model: ControlModel
    let selected: Bool
    var didSelect: (() -> Void)?
    var runtimeState: LibraryDashboardRuntimeState? = nil

    var body: some View {
        HStack(spacing: 8) {
            if let session = library.hvfRuntimeSessions.existingSession(for: config) {
                LibraryObservedWorkspaceRow(session: session, row: self)
            } else { selectionButton }
            LibraryVMActionsMenu(library: library, config: config, size: CGSize(width: 30, height: 30))
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(selected ? Color.blue.opacity(0.08) : LibraryAppearance.surface,
                    in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(selected ? Color.blue : Color.primary.opacity(0.09), lineWidth: selected ? 2 : 1)
                .allowsHitTesting(false)
        }
        .contextMenu { LibraryVMContextMenu(library: library, config: config) }
    }

    fileprivate var selectionButton: some View {
        let status = self.status
        return Button(action: select) {
            HStack(spacing: 16) {
                LibraryVMEmblem(engine: config.engineKind)
                VStack(alignment: .leading, spacing: 7) {
                    Text(config.name).font(.title3.weight(.semibold)).lineLimit(1)
                    HStack(spacing: 7) {
                        LibraryVMRunState(text: status.text, color: status.color,
                            session: library.hvfRuntimeSessions.existingRecord(slug: config.slug)?.session, dot: 8)
                    }
                    .font(.callout.weight(.medium))
                    Text(resourceText)
                        .font(.callout).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 4)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(config.name), \(status.text), \(resourceText)")
        .accessibilityHint("Show VM details")
    }

    private func select() {
        library.proMode = false
        library.selectedID = config.slug
        didSelect?()
    }

    var status: (text: String, color: Color) {
        if let entry = library.windowsInstallSessions.record(for: config.slug),
           entry.session.isRunning || (config.installPending == true && entry.sourceConfig == config) {
            switch entry.session.stage {
            case .idle: break
            case .failed: return ("Installation failed", .red)
            case .cancelled: return ("Installation canceled", .secondary)
            case .done: return ("Installation complete", .green)
            default: return (entry.session.stage.label, .blue)
            }
        }
        if config.installPending == true { return ("Installation required", .orange) }
        if let runtimeState { return (runtimeState.title, runtimeState.color) }
        return model.running ? ("Running", .green) : ("Stopped", .secondary)
    }

    private var resourceText: String {
        let cpu = config.cpuCount.map { "\($0) vCPU" } ?? "CPU: Automatic"
        let memory = config.memMiB.map { $0.isMultiple(of: 1024) ? "\($0 / 1024) GiB RAM" : "\($0) MiB RAM" } ?? "Memory: Automatic"
        return "\(cpu)  ·  \(memory)  ·  \(config.engineShortLabel)"
    }
}

/// The row label and its accessibility summary follow the same session as the detail.
private struct LibraryObservedWorkspaceRow: View {
    @ObservedObject var session: HvfEngineSession
    let row: LibraryWorkspaceRow

    var body: some View {
        var presentation = row
        presentation.runtimeState = LibraryDashboardRuntimeState(session: session, observedRunning: row.model.running)
        return presentation.selectionButton
    }
}
