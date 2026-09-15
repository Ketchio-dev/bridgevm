import SwiftUI
#if canImport(AppKit)
import AppKit
#endif
// MARK: - Root (library + detail)
struct ContentView: View {
    @ObservedObject var library: LibraryModel
    @State private var showPalette = false

    var body: some View {
        NavigationSplitView {
            LibrarySidebar(library: library)
                .frame(minWidth: 240)
        } detail: {
            LibraryDetailView(library: library)
        }
        .sheet(isPresented: $library.showingCreate) {
            CreateVMSheet(library: library)
        }
        .sheet(isPresented: $showPalette) {
            CommandPaletteView(library: library)
        }
        .sheet(item: $library.pendingWindowsClone) { cfg in
            CloneWindowsHVFSheet(library: library, config: cfg)
        }
        .confirmationDialog(
            "VM을 삭제하시겠습니까?",
            isPresented: Binding(
                get: { library.pendingDeletion != nil },
                set: { if !$0 { library.pendingDeletion = nil } }
            ),
            titleVisibility: .visible
        ) {
            if let cfg = library.pendingDeletion {
                let removesBundle = library.deletionImpact(for: cfg) == .managedBundleDeleted
                Button(removesBundle ? "\(cfg.name) 및 가상 디스크 삭제" : "\(cfg.name) 등록 제거", role: .destructive) {
                    library.confirmDeletion(cfg)
                }
            }
            Button("취소", role: .cancel) { library.pendingDeletion = nil }
        } message: {
            if let cfg = library.pendingDeletion {
                if library.deletionImpact(for: cfg) == .managedBundleDeleted {
                    Text("실행 중이면 먼저 안전하게 정지합니다. VM 설정과 \(cfg.bundlePath)에 있는 가상 디스크 데이터가 모두 영구 삭제되며 되돌릴 수 없습니다.")
                } else {
                    Text("실행 중이면 먼저 안전하게 정지합니다. BridgeVM 라이브러리 등록만 제거합니다. 외부 번들 \(cfg.bundlePath)의 가상 디스크 데이터는 유지됩니다.")
                }
            }
        }
        .modifier(LibraryOperationAlerts(library: library))
        .background(
            Button("") { showPalette = true }
                .keyboardShortcut("k", modifiers: .command)
                .opacity(0)
        )
    }
}

private struct CloneWindowsHVFSheet: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Windows HVF VM 복제").font(.title2.bold())
            Text("APFS copy-on-write로 번들을 복제합니다. 복제본은 원본의 TPM 키를 공유하지 않으며 새 TPM ID로 시작합니다. 복사된 이전 TPM 상태와 실행 증거는 보관됩니다.")
                .foregroundColor(.secondary)
            TextField("새 VM 이름", text: $name)
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("취소") { dismiss() }
                Button("복제") {
                    library.cloneWindowsHVF(config, name: name)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(VMLibrary.normalizedVMName(name) == nil)
            }
        }
        .padding(20)
        .frame(width: 480)
        .onAppear { name = "\(config.name) Copy" }
    }
}

struct VMRow: View {
    @ObservedObject var model: ControlModel
    var body: some View {
        HStack(spacing: 8) {
            Circle().fill(model.running ? Color.green : Color.gray.opacity(0.5)).frame(width: 9, height: 9)
            VStack(alignment: .leading, spacing: 1) {
                Text(model.config.name).font(.body)
                Text(model.config.engineShortLabel + " · " + (model.running ? "실행 중" : "정지"))
                    .font(.caption).foregroundColor(.secondary)
            }
            Spacer()
        }
        .padding(.vertical, 2)
    }
}

// MARK: - Pro Mode fleet table (VMware-style overview)

struct FleetTableView: View {
    @ObservedObject var library: LibraryModel
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("전체 VM (Pro)").font(.title2.bold()).padding()
            Table(library.vms) {
                TableColumn("이름") { cfg in Text(cfg.name) }
                TableColumn("엔진") { cfg in Text(cfg.engineShortLabel) }
                TableColumn("상태") { cfg in FleetStatusCell(model: library.model(for: cfg)) }
                TableColumn("부팅") { cfg in Text(cfg.effectiveBootMode) }
                TableColumn("RAM/CPU") { cfg in FleetResCell(model: library.model(for: cfg)) }
                TableColumn("IP") { cfg in FleetIPCell(model: library.model(for: cfg)) }
            }
        }
    }
}

struct FleetStatusCell: View {
    @ObservedObject var model: ControlModel
    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(model.running ? Color.green : Color.gray).frame(width: 8, height: 8)
            Text(model.running ? "실행 중" : "정지")
        }
    }
}
struct FleetResCell: View {
    @ObservedObject var model: ControlModel
    var body: some View { Text("\(Int(model.memGiB))GB · \(model.cpu)C").font(.callout.monospaced()) }
}
struct FleetIPCell: View {
    @ObservedObject var model: ControlModel
    var body: some View { Text(model.ip).font(.callout.monospaced()) }
}

// MARK: - Per-VM detail panel

struct VMDetailPanel: View {
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
                    integrationBadge("클립보드", model.backend.supportsClipboard)
                    integrationBadge("SSH", model.backend.supportsSSH)
                    integrationBadge("NAT 네트워크", true)
                }
                Divider()
                HStack {
                    Text(model.running ? "✓ 실행 중 — 세부 설정은 Advanced 탭에서" : "정지됨 — 시작하면 창이 열립니다")
                        .font(.callout).foregroundColor(.secondary)
                    Spacer()
                    Button("Advanced…") { lens = .advanced }
                }
            }.padding(6)
        } label: { Label("요약", systemImage: "sparkles") }
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
                detailRow("엔진", model.config.engineDetailLabel)
                detailRow("부팅 모드", model.config.effectiveBootMode)
                detailRow("번들", model.config.bundlePath)
                detailRow("제어", model.backend.supportsSSH
                          ? "SSH · \(model.config.sshUser)@\(model.ip)"
                          : (model.backend.supportsGuestCommands ? "BVAGENT · virtio-console" : "없음"))
                Divider()
                Button(role: .destructive) { library.requestDeletion(model.config) } label: {
                    Label("이 VM 삭제", systemImage: "trash")
                }
                .disabled(library.deletingSlugs.contains(model.config.slug))
            }.padding(6)
        } label: { Label("상세 / 관리", systemImage: "gearshape") }
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
                    Text(model.running ? "실행 중" : "정지됨").font(.headline)
                    Spacer()
                }
                HStack(spacing: 24) {
                    infoItem("IP 주소", model.ip)
                    infoItem("메모리", String(format: "%.0f GB", model.memGiB))
                    infoItem("CPU", "\(model.cpu) 코어")
                }
                HStack(spacing: 10) {
                    Button(action: model.start) { Label("시작 / 창 열기", systemImage: "play.fill") }.disabled(model.running || model.lifecycleBusy)
                    Button(action: model.stop) { Label("정지", systemImage: "stop.fill") }.disabled(!model.running || model.lifecycleBusy)
                    Button(action: model.refresh) { Label("새로고침", systemImage: "arrow.clockwise") }
                    if model.busy || model.lifecycleBusy { ProgressView().scaleEffect(0.6) }
                    Spacer()
                }
                if !model.statusNote.isEmpty {
                    Text(model.statusNote).font(.caption).foregroundColor(.secondary)
                }
            }.padding(6)
        } label: { Label("상태", systemImage: "desktopcomputer") }
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
                    Text("메모리").frame(width: 60, alignment: .leading)
                    Slider(
                        value: $model.pendingMemGiB,
                        in: Double(VMResourceLimits.minimumMemoryMiB) / 1024...Double(VMResourceLimits.maximumMemoryMiB) / 1024,
                        step: 1
                    )
                    Text("\(Int(model.pendingMemGiB)) GB").frame(width: 60, alignment: .trailing).font(.body.monospaced())
                }
                HStack {
                    Text("CPU").frame(width: 60, alignment: .leading)
                    Slider(
                        value: $model.pendingCPU,
                        in: Double(VMResourceLimits.minimumCPU)...Double(VMResourceLimits.maximumCPU),
                        step: 1
                    )
                    Text("\(Int(model.pendingCPU)) 코어").frame(width: 60, alignment: .trailing).font(.body.monospaced())
                }
                HStack {
                    Button(action: model.applyResources) { Label("적용 (VM 재시작)", systemImage: "checkmark.circle") }
                        .disabled(model.busy || model.lifecycleBusy)
                    Text("적용하면 VM이 재시작됩니다").font(.caption).foregroundColor(.secondary)
                    Spacer()
                }
                }.padding(6)
            } else {
                Label("이 엔진은 앱에서 CPU/RAM 변경을 지원하지 않습니다.", systemImage: "info.circle")
                    .foregroundColor(.secondary)
                    .padding(6)
            }
        } label: { Label("리소스 설정", systemImage: "slider.horizontal.3") }
        .onAppear {
            model.pendingMemGiB = max(1, model.memGiB)
            model.pendingCPU = Double(max(1, model.cpu))
        }
    }

    private var softwareCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text("게스트에 소프트웨어 설치 (SSH · apt)").font(.callout)
                HStack {
                    Button("브라우저 (Epiphany)") { model.installPackages(["epiphany-browser"], label: "Epiphany 브라우저") }
                    Button("텍스트 에디터 (gedit)") { model.installPackages(["gedit"], label: "gedit") }
                    Button("개발도구 (git)") { model.installPackages(["git", "build-essential"], label: "개발도구") }
                }.disabled(model.busy || model.lifecycleBusy || !model.running)
                if !model.softwareLog.isEmpty { logBox(model.softwareLog, height: 120) }
            }.padding(6)
        } label: { Label("소프트웨어", systemImage: "shippingbox") }
    }

    private var terminalCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                logBox(model.terminalLog, height: 200)
                HStack {
                    Text("dev@guest$").font(.body.monospaced()).foregroundColor(.secondary)
                    TextField("명령 입력 후 Enter", text: $model.terminalInput, onCommit: model.runTerminalCommand)
                        .textFieldStyle(.roundedBorder).font(.body.monospaced())
                        .disabled(model.busy || model.lifecycleBusy || !model.running)
                    Button("실행", action: model.runTerminalCommand).disabled(model.busy || model.lifecycleBusy || !model.running)
                }
            }.padding(6)
        } label: { Label("게스트 터미널", systemImage: "terminal") }
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
