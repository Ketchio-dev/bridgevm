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
                hero
                actionStrip
                configurationCard
                runtimeCard
                quickActions
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
                HStack(spacing: 8) {
                    Text(config.name).font(.system(size: 27, weight: .bold)).lineLimit(2)
                    Spacer(minLength: 0)
                    Button { showingAdvanced = true } label: { Image(systemName: "slider.horizontal.3") }
                        .buttonStyle(.plain).help("고급 제어 열기")
                }
                HStack(spacing: 7) {
                    LibraryVMRunState(text: statusText, color: statusColor,
                        session: library.hvfRuntimeSessions.existingRecord(slug: config.slug)?.session, dot: 10)
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
            LibraryDashboardLegacyControls(model: model, showAdvanced: { showingAdvanced = true })
        }
    }

    private var configurationCard: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("구성").font(.headline)
            LazyVGrid(columns: [GridItem(.flexible(), alignment: .leading), GridItem(.flexible(), alignment: .leading)], alignment: .leading, spacing: 20) {
                metric("프로세서", value: "\(model.cpu) vCPU", symbol: "cpu")
                metric("메모리", value: memoryText, symbol: "memorychip")
                metric("디스플레이", value: displayText, symbol: "display")
                metric("엔진", value: config.engineShortLabel, symbol: "cube")
            }
        }
        .padding(20).modifier(LibraryCardSurface())
    }

    private var runtimeCard: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("현재 상태").font(.headline)
                Spacer()
                if model.busy || model.lifecycleBusy { ProgressView().controlSize(.small) }
            }
            HStack(spacing: 24) {
                LibraryMetadataLabel(title: "상태", value: statusText, symbol: "power")
                LibraryMetadataLabel(title: "IP 주소", value: isActive ? model.ip : "—", symbol: "network")
                Spacer(minLength: 0)
            }
            if let note = visibleStatusNote {
                Divider()
                Text(note).font(.callout).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(20).modifier(LibraryCardSurface())
    }

    private var quickActions: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("빠른 작업").font(.headline)
            HStack(spacing: 12) {
                Button { revealBundle() } label: {
                    Label("Finder에서 보기", systemImage: "folder")
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                Button { showingAdvanced = true } label: {
                    Label("고급 도구", systemImage: "ellipsis.circle")
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .buttonStyle(.bordered).controlSize(.large)
        }
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

    private func metric(_ title: String, value: String, symbol: String) -> some View {
        Label {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.caption).foregroundStyle(.secondary)
                Text(value).font(.callout.weight(.semibold)).lineLimit(2)
            }
        } icon: {
            Image(systemName: symbol).font(.title3).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .accessibilityElement(children: .combine)
    }

    private var statusText: String {
        if config.installPending == true { return "설치 필요" }
        return runtimeState?.title ?? (model.running ? "실행 중" : "정지됨")
    }

    private var isActive: Bool { runtimeState?.isActive ?? model.running }
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

    private var displayText: String {
        config.engineKind == .hvfEngine
            ? (config.retinaResolution == true ? "Retina · 창 크기에 맞춤" : "창 크기에 맞춤")
            : "\(config.displayWidth) × \(config.displayHeight)"
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
