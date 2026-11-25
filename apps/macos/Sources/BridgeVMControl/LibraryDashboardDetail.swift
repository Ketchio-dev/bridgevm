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
        .sheet(isPresented: $showingSettings) {
            if let session = settingsSession { HvfVMSettingsSheet(session: session) }
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
        .navigationTitle(config.name)
    }

    private var hero: some View {
        HStack(alignment: .top, spacing: 20) {
            LibraryVMEmblem(engine: config.engineKind)
            VStack(alignment: .leading, spacing: 8) {
                Text(config.name).font(.system(size: 27, weight: .bold)).lineLimit(2)
                HStack(spacing: 7) {
                    LibraryVMRunState(text: statusText, color: statusColor,
                        session: library.hvfRuntimeSessions.existingRecord(slug: config.slug)?.session, dot: 8)
                    if model.busy || model.lifecycleBusy { ProgressView().controlSize(.small) }
                }
                .font(.headline)
                if config.displayName != config.name {
                    Text(config.displayName).foregroundStyle(.secondary).lineLimit(2)
                }
                Text([config.engineDetailLabel, model.graphicsPresentation?.label].compactMap { $0 }.joined(separator: " · ")).font(.callout).foregroundStyle(.secondary)
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
            Text("구성").font(.headline)
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), alignment: .leading), count: 2),
                      alignment: .leading, spacing: 18) {
                LibraryMetadataLabel(title: "프로세서", value: "\(model.cpu) vCPU", symbol: "cpu")
                LibraryMetadataLabel(title: "메모리", value: memoryText, symbol: "memorychip")
                LibraryMetadataLabel(title: "디스플레이", value: Self.displayText(for: config), symbol: "display")
                if config.engineKind == .hvfEngine {
                    LibraryMetadataLabel(title: "네트워크", value: config.networkEnabled == false ? "꺼짐" : "공유 네트워크 (NAT)",
                                         symbol: "network")
                }
            }
            Divider()
            HStack(spacing: 12) {
                Image(systemName: "folder").foregroundStyle(.secondary).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text("저장 위치").font(.caption).foregroundStyle(.secondary)
                    Text(verbatim: (config.bundlePath as NSString).abbreviatingWithTildeInPath)
                        .font(.callout).lineLimit(1).truncationMode(.middle)
                        .textSelection(.enabled).help(config.bundlePath)
                }
                .accessibilityElement(children: .combine)
                Spacer(minLength: 0)
                Button("Finder에서 보기", action: revealBundle)
                    .fixedSize()
            }
        }
        .padding(20).modifier(LibraryCardSurface())
    }

    @ViewBuilder private var advancedView: some View {
        VStack(spacing: 0) {
            HStack {
                Button { showingAdvanced = false } label: {
                    Label("대시보드", systemImage: "chevron.left")
                }
                .accessibilityIdentifier("bridgevm.dashboard.back")
                Spacer()
                Text("고급 제어").font(.headline)
                Spacer()
                Color.clear.frame(width: 80, height: 1)
            }
            .padding(.horizontal, 20).padding(.vertical, 12)
            Divider()
            LibraryRuntimeDetailContent(config: config, model: model, library: library)
        }
    }

    private var statusText: String {
        if config.installPending == true { return "설치 필요" }
        return runtimeState?.title ?? (model.running ? "실행 중" : "정지됨")
    }

    private var visibleStatusNote: String? {
        if let runtimeState { return runtimeState.note(model.statusNote) }
        return model.statusNote.isEmpty ? nil : model.statusNote
    }

    private var statusColor: Color {
        if config.installPending == true { return .orange }
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
            return config.retinaResolution == true ? "Retina · 창 크기에 맞춤" : "창 크기에 맞춤"
        }
        if let mode = HvfGuestResolutionPicker.firmwareMode(config.guestResolution) {
            return "부팅 시 " + mode.replacingOccurrences(of: "x", with: " × ")
        }
        // Automatic preserves an existing firmware choice. A supplied invalid mode leaves it untouched.
        return config.guestResolution == nil ? "부팅 시 자동 (기본 1280 × 800)" : "부팅 시 펌웨어 설정"
    }

    private func revealBundle() {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: config.bundlePath)])
    }
}

struct DashboardActionButton: View {
    static let columns = [GridItem(.adaptive(minimum: 140), spacing: 10)]
    let title: String
    let symbol: String
    var prominent = false
    var disabled = false
    let action: () -> Void

    var body: some View {
        Group {
            if prominent { actionButton.buttonStyle(.borderedProminent) }
            else { actionButton.buttonStyle(.bordered) }
        }
        .controlSize(.large)
        .disabled(disabled)
    }

    private var actionButton: some View {
        Button(action: action) {
            Label {
                Text(title).font(.callout.weight(.medium)).fixedSize()
            } icon: {
                Image(systemName: symbol).font(.body)
            }
            .frame(maxWidth: .infinity, minHeight: 28)
            .padding(.vertical, 4)
        }
    }
}
