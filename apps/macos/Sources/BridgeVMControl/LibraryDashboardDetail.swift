import SwiftUI
import AppKit

struct LibraryDashboardRoute: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if let detail = library.selectedDetail,
           !library.shouldShowWindowsInstall(for: detail.config) {
            LibraryDashboardDetail(config: detail.config, model: detail.model, library: library)
                .id(detail.config.slug)
        } else {
            LibraryDetailView(library: library)
        }
    }
}

struct LibraryDashboardDetail: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel
    @State private var showingAdvanced = false

    var body: some View {
        if showingAdvanced {
            advancedView
        } else {
            dashboard
        }
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
            .frame(maxWidth: 850, alignment: .topLeading)
            .frame(maxWidth: .infinity, alignment: .top)
        }
        .background(LibraryAppearance.canvas)
        .navigationTitle(config.name)
    }

    private var hero: some View {
        HStack(alignment: .top, spacing: 20) {
            ZStack {
                RoundedRectangle(cornerRadius: 14)
                    .fill(LinearGradient(colors: heroColors, startPoint: .topLeading, endPoint: .bottomTrailing))
                LibraryVMEmblem(engine: config.engineKind)
                    .scaleEffect(1.25)
            }
            .frame(width: 190, height: 120)
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Text(config.name).font(.system(size: 27, weight: .bold)).lineLimit(2)
                    Spacer(minLength: 0)
                    Button { showingAdvanced = true } label: { Image(systemName: "slider.horizontal.3") }
                        .buttonStyle(.plain).help("고급 제어 열기")
                }
                HStack(spacing: 7) {
                    Circle().fill(statusColor).frame(width: 10, height: 10)
                    Text(statusText).foregroundStyle(statusColor)
                }
                .font(.headline)
                Text(config.displayName).foregroundStyle(.secondary).lineLimit(2)
                Text(config.engineDetailLabel).font(.callout).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var actionStrip: some View {
        HStack(spacing: 12) {
            DashboardActionButton(
                title: model.running ? "창 열기" : "시작",
                symbol: "play.fill", prominent: true,
                disabled: model.lifecycleBusy,
                action: { LibraryDashboardPrimaryAction.perform(config: config, model: model, library: library) }
            )
            DashboardActionButton(
                title: "정지", symbol: "stop.fill",
                disabled: !model.running || model.lifecycleBusy,
                action: model.stop
            )
            DashboardActionButton(title: "새로고침", symbol: "arrow.clockwise", action: model.refresh)
            if config.engineKind == .hvfEngine {
                DashboardActionButton(
                    title: "복제", symbol: "plus.square.on.square",
                    disabled: model.running || library.cloningSlugs.contains(config.slug)
                ) { library.requestWindowsClone(config) }
            }
            DashboardActionButton(title: "고급", symbol: "gearshape") { showingAdvanced = true }
        }
    }

    private var configurationCard: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("구성").font(.headline)
            HStack(spacing: 0) {
                metric("프로세서", value: "\(model.cpu) vCPU", symbol: "cpu")
                Divider().frame(height: 50)
                metric("메모리", value: memoryText, symbol: "memorychip")
                Divider().frame(height: 50)
                metric("디스플레이", value: displayText, symbol: "display")
                Divider().frame(height: 50)
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
                LibraryMetadataLabel(title: "IP 주소", value: model.running ? model.ip : "—", symbol: "network")
                Spacer(minLength: 0)
            }
            if !model.statusNote.isEmpty {
                Divider()
                Text(model.statusNote).font(.callout).foregroundStyle(.secondary)
            }
            Text("CPU·메모리 사용률은 게스트 측 측정값이 연결된 뒤 표시됩니다.")
                .font(.caption).foregroundStyle(.secondary)
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
                    Label("전체 설정 및 도구", systemImage: "ellipsis.circle")
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
                Spacer()
                Text("고급 제어").font(.headline)
                Spacer()
                Color.clear.frame(width: 80, height: 1)
            }
            .padding(.horizontal, 20).padding(.vertical, 12)
            Divider()
            if let session = library.hvfRuntimeDetailSession(for: config) {
                HvfEngineView(session: session)
            } else {
                VMDetailPanel(model: model, library: library)
            }
        }
    }

    private func metric(_ title: String, value: String, symbol: String) -> some View {
        Label {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.caption).foregroundStyle(.secondary)
                Text(value).font(.callout.weight(.semibold)).lineLimit(1)
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
        return model.running ? "실행 중" : "정지됨"
    }

    private var statusColor: Color {
        if config.installPending == true { return .orange }
        return model.running ? .green : .secondary
    }

    private var memoryText: String {
        model.memGiB.rounded() == model.memGiB
            ? "\(Int(model.memGiB)) GiB"
            : String(format: "%.1f GiB", model.memGiB)
    }

    private var displayText: String { "\(config.displayWidth) × \(config.displayHeight)" }

    private var heroColors: [Color] {
        config.engineKind == .hvfEngine
            ? [Color.blue.opacity(0.16), Color.purple.opacity(0.08)]
            : [Color.teal.opacity(0.17), Color.blue.opacity(0.07)]
    }

    private func revealBundle() {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: config.bundlePath)])
    }
}

private struct DashboardActionButton: View {
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
        .disabled(disabled)
    }

    private var actionButton: some View {
        Button(action: action) {
            VStack(spacing: 8) {
                Image(systemName: symbol).font(.title3)
                Text(title).font(.callout)
            }
            .frame(maxWidth: .infinity).frame(height: 64)
        }
    }
}
