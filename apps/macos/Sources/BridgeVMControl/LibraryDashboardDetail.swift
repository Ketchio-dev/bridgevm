import SwiftUI
import AppKit

struct LibraryDashboardRoute: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if !library.proMode, let detail = library.selectedDetail,
           !library.shouldShowWindowsInstall(for: detail.config) {
            let dashboard = LibraryDashboardDetail(config: detail.config, model: detail.model, library: library)
            if let session = library.hvfRuntimeDetailSession(for: detail.config) {
                LibraryDashboardObservedRuntime(session: session, detail: dashboard).id(detail.config.slug)
            } else {
                dashboard.id(detail.config.slug)
            }
        } else {
            LibraryDetailView(library: library)
        }
    }
}

struct LibraryDashboardDetail: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel
    var runtimeState: LibraryDashboardRuntimeState? = nil
    @State private var showingAdvanced = false
    @State private var showingSettings = false

    var body: some View {
        Group {
            if showingAdvanced {
                advancedView
            } else {
                dashboard
            }
        }
        .focusedSceneValue(\.hvfVMSettings, settingsSession == nil ? nil : { showingSettings = true })
        .sheet(isPresented: $showingSettings) { settingsSheet }
    }

    @ViewBuilder var settingsSheet: some View {
        if let session = settingsSession {
            HvfVMSettingsSheet(session: session).id(ObjectIdentifier(session))
        }
    }

    private var settingsSession: HvfEngineSession? {
        config.engineKind == .hvfEngine ? library.hvfRuntimeDetailSession(for: config) : nil
    }

    private var dashboard: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 16) {
                    hero
                    if let note = visibleStatusNote {
                        Text(note).font(.callout).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .textSelection(.enabled)
                    }
                    // A Quit waits for this VM's move, duplicate or delete; it can be called off here.
                    if library.fileOperationTitle(for: config.slug) != nil { VMPreparationQuitStatus(library: library) }
                    actionStrip
                }
                .padding(20).modifier(LibraryCardSurface())
                configurationCard
            }
            .padding(24)
            .frame(maxWidth: 760, alignment: .topLeading)
            .frame(maxWidth: .infinity, alignment: .top)
        }
        .background(LibraryAppearance.canvas)
        // The VM window has the VM's name: two windows of that name in the Window menu and Mission Control.
        .navigationTitle("Library")
        .navigationSubtitle(config.name)
    }

    private var hero: some View {
        HStack(alignment: .top, spacing: 20) {
            LibraryVMEmblem(engine: config.engineKind)
            VStack(alignment: .leading, spacing: 8) {
                Text(config.name).font(.system(size: 27, weight: .bold)).lineLimit(2)
                HStack(spacing: 7) {
                    LibraryVMRunState(text: statusText, color: statusColor,
                        session: library.hvfRuntimeSessions.existingRecord(slug: config.slug)?.session, dot: 8)
                }
                .font(.headline)
                if config.displayName != config.name {
                    Text(config.displayName).foregroundStyle(.secondary).lineLimit(2)
                }
                Text([config.engineDetailLabel, graphicsLabel].compactMap { $0 }.joined(separator: " · ")).font(.callout).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @ViewBuilder private var actionStrip: some View {
        if config.engineKind == .hvfEngine {
            if let session = settingsSession {
                LibraryDashboardRuntimeControls(config: config, model: model,
                    library: library, session: session,
                    showAdvanced: { showingAdvanced = true }, showSettings: { showingSettings = true })
            } else {
                LibraryDashboardUnavailableRuntimeControls(config: config, model: model,
                    library: library, showAdvanced: { showingAdvanced = true })
            }
        } else {
            Label(BackendKind.unsupportedNotice, systemImage: "exclamationmark.triangle")
                .font(.callout).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("bridgevm.dashboard.unsupported-engine")
        }
    }

    private var configurationCard: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Configuration").font(.headline)
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), alignment: .leading), count: 2),
                      alignment: .leading, spacing: 18) {
                LibraryMetadataLabel(title: "Processor", value: "\(model.cpu) vCPU", symbol: "cpu")
                LibraryMetadataLabel(title: "Memory", value: memoryText, symbol: "memorychip")
                LibraryMetadataLabel(title: "Display", value: Self.displayText(for: config), symbol: "display")
                if config.engineKind == .hvfEngine {
                    LibraryMetadataLabel(title: "Network", value: config.networkEnabled == false ? "Off" : "Shared network (NAT)",
                                         symbol: "network")
                }
            }
            Divider()
            HStack(spacing: 12) {
                Image(systemName: "folder").foregroundStyle(.secondary).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Location").font(.caption).foregroundStyle(.secondary)
                    Text(verbatim: (config.bundlePath as NSString).abbreviatingWithTildeInPath)
                        .font(.callout).lineLimit(1).truncationMode(.middle)
                        .textSelection(.enabled).help(config.bundlePath)
                }
                .accessibilityElement(children: .combine)
                Spacer(minLength: 0)
                Button("Show in Finder") {
                    library.revealBundleInFinder(config) { NSWorkspace.shared.activateFileViewerSelecting($0) }
                }
                .fixedSize()
            }
        }
        .padding(20).modifier(LibraryCardSurface())
    }

    @ViewBuilder private var advancedView: some View {
        VStack(spacing: 0) {
            HStack {
                Button { showingAdvanced = false } label: {
                    Label("Dashboard", systemImage: "chevron.left")
                }
                .accessibilityIdentifier("bridgevm.dashboard.back")
                Spacer()
                Text("Advanced Controls").font(.headline)
                Spacer()
                Color.clear.frame(width: 80, height: 1)
            }
            .padding(.horizontal, 20).padding(.vertical, 12)
            Divider()
            LibraryRuntimeDetailContent(config: config, library: library)
        }
    }

    private var graphicsLabel: String? {
        guard config.engineKind == .hvfEngine else { return nil }
        let session = library.hvfRuntimeSessions.existingRecord(slug: config.slug)?.session
        return HvfGraphicsRuntimePresentation.classify(running: model.running,
            ownedRun3D: session?.ownedRunExperimental3D, saved3D: config.experimental3DAllowed).label
    }

    var statusText: String {
        if config.installPending == true { return "Installation required" }
        if let operation = library.fileOperationTitle(for: config.slug) { return operation }
        return runtimeState?.title ?? (model.running ? "Running" : "Stopped")
    }

    var visibleStatusNote: String? {
        if let runtimeState { return runtimeState.note(model.statusNote) }
        return model.statusNote.isEmpty ? nil : model.statusNote
    }

    private var statusColor: Color {
        if config.installPending == true { return .orange }
        if library.fileOperationTitle(for: config.slug) != nil { return .blue }
        return runtimeState?.color ?? (model.running ? .green : .secondary)
    }

    private var memoryText: String {
        model.memGiB.rounded() == model.memGiB
            ? "\(Int(model.memGiB)) GiB"
            : String(format: "%.1f GiB", model.memGiB)
    }

    static func displayText(for config: VMConfig) -> String {
        guard config.engineKind == .hvfEngine else {
            return "\(config.displayWidth) × \(config.displayHeight)"
        }
        if HvfEngineConfig.virtioGpu2d(for: config) {
            return config.retinaResolution == true ? "Retina · Fit to window" : "Fit to window"
        }
        if let mode = HvfGuestResolutionPicker.firmwareMode(config.guestResolution) {
            return mode.replacingOccurrences(of: "x", with: " × ") + " at startup"
        }
        // Automatic preserves an existing firmware choice. A supplied invalid mode leaves it untouched.
        return config.guestResolution == nil ? "Automatic at startup (1280 × 800 by default)" : "Firmware setting at startup"
    }
}
