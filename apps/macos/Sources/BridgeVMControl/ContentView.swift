import SwiftUI
#if canImport(AppKit)
import AppKit
#endif
struct ContentView: View {
    @ObservedObject var library: LibraryModel
    @State private var presentation = LibraryWindowPresentation()
    @State private var importDraft = FirstRunFormDraft()
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    var body: some View {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        let _ = AppUIHost.prepared?.lifecycle.record(.contentBodyEvaluated)
        #endif
        GeometryReader { geometry in
            if geometry.size.width < 850 {
                CompactLibraryNavigation(library: library)
            } else {
                let wide = geometry.size.width >= 1100
                libraryNavigation
                    .onChange(of: wide, initial: true) { _, wide in
                        columnVisibility = wide ? .all : .doubleColumn
                    }
            }
        }
        .environment(\.firstRunFormDraft, $importDraft)
        .onChange(of: library.selectedID) { _, selection in
            if let selection, selection != LibraryModel.firstRunImportSelectionID {
                importDraft = FirstRunFormDraft()
            }
        }
        .tint(LibraryAppearance.accent)
        .groupBoxStyle(LibraryGroupBoxStyle())
        .background(LibraryAppearance.canvas)
        .environment(\.libraryWindowPresentation, $presentation)
        .sheet(item: $presentation.sheet) { sheet in
            switch sheet {
            case .palette:
                CommandPaletteView(library: library) { presentation.sheet = .create(installer: nil) }
            case .create(let installer):
                CreateVMSheet(library: library, isoPath: installer?.path ?? "")
            }
        }
        .sheet(item: $presentation.cloneSource) { cfg in
            CloneWindowsHVFSheet(library: library, config: cfg)
        }
        .confirmationDialog(
            "Remove this virtual machine?",
            isPresented: Binding(
                get: { presentation.deletionTarget != nil },
                set: { if !$0 { presentation.deletionTarget = nil } }
            ),
            titleVisibility: .visible,
            presenting: presentation.deletionTarget
        ) { config in
            Button(LibraryDeletionPrompt(config: config, rootURL: library.rootURL).confirmTitle, role: .destructive) {
                library.confirmDeletion(config)
            }
            Button("Cancel", role: .cancel) { }
        } message: { config in
            Text(LibraryDeletionPrompt(config: config, rootURL: library.rootURL).message)
        }
        .modifier(LibraryOperationAlerts(library: library))
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { presentation.sheet = .palette } label: {
                    Label("Command Palette", systemImage: "magnifyingglass")
                }
                .keyboardShortcut("k", modifiers: .command)
                .help("Search commands or VMs (⌘K)")
                .accessibilityIdentifier("bridgevm.library.toolbar.palette")
            }
        }
    }

    @ViewBuilder
    private var libraryNavigation: some View {
        if library.vms.isEmpty && library.libraryIssues.isEmpty {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                sidebar
            } detail: {
                LibraryDashboardRoute(library: library)
            }
        } else {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                sidebar
            } content: {
                LibraryOverviewView(library: library)
                    .navigationSplitViewColumnWidth(min: 390, ideal: 480, max: 580)
            } detail: {
                LibraryDashboardRoute(library: library)
            }
        }
    }

    private var sidebar: some View {
        LibrarySidebar(library: library)
            .navigationSplitViewColumnWidth(min: 220, ideal: 240, max: 270)
    }
}

struct VMDetailPanel: View {
    @Environment(\.libraryWindowPresentation) private var presentation
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel

    enum Lens: String { case simple, advanced }
    @AppStorage("bridgevm.defaultLens") private var lens: Lens = .simple

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                statusCard
                if lens == .advanced {
                    resourcesCard
                    if model.backend.supportsPackageInstall {
                        softwareCard
                    }
                    if model.backend.supportsGuestCommands {
                        terminalCard
                    }
                    detailsCard
                } else {
                    simpleExtras
                }
            }
            .padding(20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .navigationTitle(model.config.name)
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text(model.config.name).font(.largeTitle.bold())
                Text(model.displayName).foregroundColor(.secondary)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 8) {
                engineChip
                Picker("", selection: $lens) {
                    Text("Simple").tag(Lens.simple)
                    Text("Advanced").tag(Lens.advanced)
                }
                .pickerStyle(.segmented).frame(width: 190).labelsHidden()
            }
        }
    }

    private var simpleExtras: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 18) {
                    integrationBadge("Clipboard", model.backend.supportsClipboard)
                    integrationBadge("SSH", model.backend.supportsSSH)
                    integrationBadge("NAT network", true)
                }
                Divider()
                HStack {
                    Text(model.running ? "Running. Open Advanced for more settings." : "Stopped. Start the VM to open its window.")
                        .font(.callout).foregroundColor(.secondary)
                    Spacer()
                    Button("Advanced…") { lens = .advanced }
                }
            }.padding(6)
        } label: { Label("Summary", systemImage: "sparkles") }
    }

    private func integrationBadge(_ t: String, _ on: Bool) -> some View {
        HStack(spacing: 5) {
            Image(systemName: on ? "checkmark.circle.fill" : "minus.circle").foregroundColor(on ? .green : .secondary)
            Text(t).font(.callout)
        }
    }

    private var detailsCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                detailRow("Engine", model.config.engineDetailLabel)
                detailRow("Boot mode", model.config.effectiveBootMode)
                detailRow("Bundle", model.config.bundlePath)
                detailRow("Control", model.backend.supportsSSH
                          ? "SSH · \(model.config.sshUser)@\(model.ip)"
                          : (model.backend.supportsGuestCommands ? "BVAGENT · virtio-console" : "None"))
                Divider()
                Button(role: .destructive) { presentation?.wrappedValue.deletionTarget = library.requestDeletion(model.config) } label: {
                    Label("Remove this VM…", systemImage: "trash")
                }
                .disabled(model.running || library.deletingSlugs.contains(model.config.slug))
            }.padding(6)
        } label: { Label("Details and management", systemImage: "gearshape") }
    }

    private func detailRow(_ k: String, _ v: String) -> some View {
        HStack(alignment: .top) {
            Text(k).foregroundColor(.secondary).frame(width: 70, alignment: .leading)
            Text(v).font(.caption.monospaced()).textSelection(.enabled)
            Spacer()
        }
    }

    private var engineChip: some View {
        Text(model.config.engineDetailLabel)
            .font(.callout)
            .padding(.horizontal, 12).padding(.vertical, 6)
            .background(Color.accentColor.opacity(0.18))
            .cornerRadius(8)
    }

    private var statusCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    Circle().fill(model.running ? Color.green : Color.red).frame(width: 12, height: 12)
                    Text(model.running ? "Running" : "Stopped").font(.headline)
                    Spacer()
                }
                HStack(spacing: 24) {
                    infoItem("IP address", model.ip)
                    infoItem("Memory", String(format: "%.0f GiB", model.memGiB))
                    infoItem("CPU", "\(model.cpu) cores")
                }
                HStack(spacing: 10) {
                    Button(action: model.start) { Label("Start / Open Window", systemImage: "play.fill") }.disabled(model.running || model.lifecycleBusy)
                    Button(action: model.stop) { Label("Stop", systemImage: "stop.fill") }.disabled(!model.running || model.lifecycleBusy)
                    Button(action: model.refresh) { Label("Refresh", systemImage: "arrow.clockwise") }
                    if model.busy || model.lifecycleBusy { ProgressView().scaleEffect(0.6) }
                    Spacer()
                }
                if !model.statusNote.isEmpty {
                    Text(model.statusNote).font(.caption).foregroundColor(.secondary)
                }
            }.padding(6)
        } label: { Label("Status", systemImage: "desktopcomputer") }
    }

    private func infoItem(_ k: String, _ v: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(k).font(.caption).foregroundColor(.secondary)
            Text(v).font(.body.monospaced())
        }
    }

    private var resourcesCard: some View {
        GroupBox {
            if model.backend.supportsResourceChanges {
                VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Memory").frame(width: 60, alignment: .leading)
                    Slider(
                        value: $model.pendingMemGiB,
                        in: Double(VMResourceLimits.minimumMemoryMiB) / 1024...Double(VMResourceLimits.maximumMemoryMiB) / 1024,
                        step: 1
                    )
                    Text("\(Int(model.pendingMemGiB)) GiB").frame(width: 60, alignment: .trailing).font(.body.monospaced())
                }
                HStack {
                    Text("CPU").frame(width: 60, alignment: .leading)
                    Slider(
                        value: $model.pendingCPU,
                        in: Double(VMResourceLimits.minimumCPU)...Double(VMResourceLimits.maximumCPU),
                        step: 1
                    )
                    Text("\(Int(model.pendingCPU)) cores").frame(width: 60, alignment: .trailing).font(.body.monospaced())
                }
                HStack {
                    Button(action: model.applyResources) { Label("Apply and Restart", systemImage: "checkmark.circle") }
                        .disabled(model.busy || model.lifecycleBusy)
                    Text("The VM restarts to apply these settings.").font(.caption).foregroundColor(.secondary)
                    Spacer()
                }
                }.padding(6)
            } else {
                Label("CPU and memory settings are read-only for this engine.", systemImage: "info.circle")
                    .foregroundColor(.secondary)
                    .padding(6)
            }
        } label: { Label("Resources", systemImage: "slider.horizontal.3") }
        .onAppear {
            model.pendingMemGiB = max(1, model.memGiB)
            model.pendingCPU = Double(max(1, model.cpu))
        }
    }

    private var softwareCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text("Install guest software with apt over SSH").font(.callout)
                HStack {
                    Button("Web browser (Epiphany)") { model.installPackages(["epiphany-browser"], label: "Epiphany browser") }
                    Button("Text editor (gedit)") { model.installPackages(["gedit"], label: "gedit") }
                    Button("Developer tools (Git)") { model.installPackages(["git", "build-essential"], label: "developer tools") }
                }.disabled(model.busy || model.lifecycleBusy || !model.running)
                if !model.softwareLog.isEmpty { logBox(model.softwareLog, height: 120) }
            }.padding(6)
        } label: { Label("Software", systemImage: "shippingbox") }
    }

    private var terminalCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                logBox(model.terminalLog, height: 200)
                HStack {
                    Text("dev@guest$").font(.body.monospaced()).foregroundColor(.secondary)
                    TextField("Enter a command and press Return", text: $model.terminalInput, onCommit: model.runTerminalCommand)
                        .textFieldStyle(.roundedBorder).font(.body.monospaced())
                        .disabled(model.busy || model.lifecycleBusy || !model.running)
                    Button("Run", action: model.runTerminalCommand).disabled(model.busy || model.lifecycleBusy || !model.running)
                }
            }.padding(6)
        } label: { Label("Guest terminal", systemImage: "terminal") }
    }

    private func logBox(_ text: String, height: CGFloat) -> some View {
        ScrollView {
            Text(text).font(.system(size: 11, design: .monospaced))
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled).padding(8)
        }
        .frame(height: height)
        .background(Color(white: 0.1)).foregroundColor(Color(white: 0.9)).cornerRadius(6)
    }
}
