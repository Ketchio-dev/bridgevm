import SwiftUI

// MARK: - Root (library + detail)

struct ContentView: View {
    @ObservedObject var library: LibraryModel
    @State private var showPalette = false

    var body: some View {
        NavigationSplitView {
            LibrarySidebar(library: library)
                .frame(minWidth: 240)
        } detail: {
            if library.proMode {
                FleetTableView(library: library)
            } else if let model = library.selectedModel {
                VMDetailPanel(model: model, library: library)
                    .id(model.config.slug)
            } else {
                emptyState
            }
        }
        .sheet(isPresented: $library.showingCreate) {
            CreateVMSheet(library: library)
        }
        .sheet(isPresented: $showPalette) {
            CommandPaletteView(library: library)
        }
        .background(
            Button("") { showPalette = true }
                .keyboardShortcut("k", modifiers: .command)
                .opacity(0)
        )
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Image(systemName: "desktopcomputer").font(.system(size: 48)).foregroundColor(.secondary)
            Text("VM을 선택하거나 새로 만드세요").foregroundColor(.secondary)
            Button { library.showingCreate = true } label: { Label("새 VM", systemImage: "plus") }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Sidebar (the VM library)

struct LibrarySidebar: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        VStack(spacing: 0) {
            List(selection: $library.selectedID) {
                Section("VM 라이브러리") {
                    ForEach(library.vms) { cfg in
                        VMRow(model: library.model(for: cfg))
                            .tag(cfg.slug)
                            .contextMenu {
                                Button(role: .destructive) { library.delete(cfg) } label: { Label("삭제", systemImage: "trash") }
                            }
                    }
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
            legendRow(.gray, "윈도우 (HVF 엔진)", "준비중")
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
                    if model.backend.supportsGuestCommands {
                        softwareCard
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
                    integrationBadge("클립보드", model.backend.supportsGuestCommands)
                    integrationBadge("SSH", model.backend.supportsGuestCommands)
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
                detailRow("SSH", "\(model.config.sshUser)@\(model.ip)")
                Divider()
                Button(role: .destructive) { library.delete(model.config) } label: {
                    Label("이 VM 삭제", systemImage: "trash")
                }
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
                    Button(action: model.start) { Label("시작 / 창 열기", systemImage: "play.fill") }.disabled(model.running)
                    Button(action: model.stop) { Label("정지", systemImage: "stop.fill") }.disabled(!model.running)
                    Button(action: model.refresh) { Label("새로고침", systemImage: "arrow.clockwise") }
                    if model.busy { ProgressView().scaleEffect(0.6) }
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
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("메모리").frame(width: 60, alignment: .leading)
                    Slider(value: $model.pendingMemGiB, in: 1...32, step: 1)
                    Text("\(Int(model.pendingMemGiB)) GB").frame(width: 60, alignment: .trailing).font(.body.monospaced())
                }
                HStack {
                    Text("CPU").frame(width: 60, alignment: .leading)
                    Slider(value: $model.pendingCPU, in: 1...10, step: 1)
                    Text("\(Int(model.pendingCPU)) 코어").frame(width: 60, alignment: .trailing).font(.body.monospaced())
                }
                HStack {
                    Button(action: model.applyResources) { Label("적용 (VM 재시작)", systemImage: "checkmark.circle") }.disabled(model.busy)
                    Text("적용하면 VM이 재시작됩니다").font(.caption).foregroundColor(.secondary)
                    Spacer()
                }
            }.padding(6)
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
                }.disabled(model.busy || !model.running)
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
                        .disabled(model.busy || !model.running)
                    Button("실행", action: model.runTerminalCommand).disabled(model.busy || !model.running)
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
