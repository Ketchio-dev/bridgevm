import SwiftUI
#if canImport(AppKit)
import AppKit
#endif

// MARK: - Sidebar (the VM library)

struct LibrarySidebar: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        VStack(spacing: 0) {
            List(selection: $library.selectedID) {
                FirstRunImportSidebarEntry(library: library)
                RetainedControlsSidebarEntry(library: library)
                Section("VM 라이브러리") {
                    ForEach(library.vms) { cfg in
                        VMRow(model: library.model(for: cfg))
                            .tag(cfg.slug)
                            .contextMenu {
                                if cfg.engineKind == .hvfEngine, cfg.installPending != true {
                                    Button { library.requestWindowsClone(cfg) } label: {
                                        Label("새 TPM ID로 복제", systemImage: "plus.square.on.square")
                                    }
                                    .disabled(library.cloningSlugs.contains(cfg.slug))
                                    Button { chooseMoveDestination(for: cfg) } label: {
                                        Label("같은 VM ID로 번들 이동", systemImage: "folder.badge.gearshape")
                                    }
                                    .disabled(library.movingSlugs.contains(cfg.slug))
                                }
                                Button(role: .destructive) { library.requestDeletion(cfg) } label: { Label("삭제", systemImage: "trash") }
                                    .disabled(library.deletingSlugs.contains(cfg.slug)
                                              || library.cloningSlugs.contains(cfg.slug)
                                              || library.movingSlugs.contains(cfg.slug))
                            }
                    }
                }
                if !library.libraryIssues.isEmpty {
                    Section("라이브러리 경고") {
                        Label("읽지 못한 VM 설정 \(library.libraryIssues.count)개", systemImage: "exclamationmark.triangle.fill")
                            .foregroundColor(.orange)
                        ForEach(Array(library.libraryIssues.prefix(3))) { issue in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(URL(fileURLWithPath: issue.path).deletingLastPathComponent().lastPathComponent)
                                    .font(.caption.bold())
                                Text(issue.message).font(.caption2).foregroundColor(.secondary)
                            }
                            .help(issue.path)
                        }
                    }
                }
                Section("실험") {
                    Label("HVF Engine", systemImage: "cpu")
                        .tag(LibraryModel.hvfEngineSelectionID)
                }
            }
            Divider()
            hostMeter.padding(10)
            Divider()
            engineLegend
                .padding(10)
        }
        .toolbar {
            ToolbarItem {
                Toggle(isOn: $library.proMode) { Label("Pro", systemImage: "tablecells") }
                    .toggleStyle(.button)
                    .help("Pro 모드 — 전체 VM 테이블")
            }
            ToolbarItem {
                Button { library.showingCreate = true } label: { Label("새 VM", systemImage: "plus") }
                    .accessibilityIdentifier("bridgevm.library.toolbar.create")
            }
        }
    }

    private var hostMeter: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("호스트 용량 (실행 중 합계)").font(.caption).foregroundColor(.secondary)
            meterRow("RAM", used: library.usedMemGiB, total: library.hostMemGiB, unit: "GB")
            meterRow("CPU", used: Double(library.usedCPU), total: Double(library.hostCPU), unit: "코어")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func meterRow(_ label: String, used: Double, total: Double, unit: String) -> some View {
        let frac = total > 0 ? min(1.0, used / total) : 0
        return VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(label).font(.caption2).foregroundColor(.secondary)
                Spacer()
                Text("\(Int(used.rounded()))/\(Int(total.rounded())) \(unit)")
                    .font(.caption2.monospaced())
                    .foregroundColor(frac > 0.9 ? .orange : .secondary)
            }
            ProgressView(value: frac).tint(frac > 0.9 ? .orange : .accentColor)
        }
    }

    private var engineLegend: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("엔진").font(.caption).foregroundColor(.secondary)
            legendRow(.green, "리눅스 (Fast VZ)", "사용 가능")
            legendRow(.gray, "윈도우 (QEMU)", "준비중")
            legendRow(.orange, "윈도우 (HVF 엔진)", "실험")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func legendRow(_ c: Color, _ t: String, _ s: String) -> some View {
        HStack(spacing: 6) {
            Circle().fill(c).frame(width: 7, height: 7)
            Text(t).font(.caption)
            Spacer()
            Text(s).font(.caption2).foregroundColor(.secondary)
        }
    }

    private func chooseMoveDestination(for config: VMConfig) {
        #if canImport(AppKit)
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "여기로 이동"
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        library.moveWindowsHVFBundle(config, to: destination)
        #endif
    }
}
