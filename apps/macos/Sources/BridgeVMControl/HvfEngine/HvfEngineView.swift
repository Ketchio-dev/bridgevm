import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif

struct HvfEngineView: View {
    @StateObject private var session: HvfEngineSession
    @State private var targetDiskPath = ""
    @State private var uefiVarsPath = ""
    @State private var evidenceDir = ""
    @State private var watchdogEnabled = false
    @State private var watchdogMs = 900_000
    @State private var ramMiB = 6144
    @State private var smpCpus = 4
    @State private var clipboardSync = true
    @State private var shareEnabled = false
    @State private var shareHostDir = ""
    @State private var shareGuestDir = "C:\\bridgevm-share"
    @State private var virtioNet = false
    @State private var virtioGpu3d = true
    @State private var nvmeBufferedIO = true
    @State private var ctlFilePath = ""
    @State private var ctlInput = ""
    @State private var keyboardInput = ""
    @State private var vtpmRecoveryCode = ""
    @State private var vtpmRecoveryPackagePath = ""
    @State private var vtpmRecoveryCodeInput = ""
    @State private var vtpmLifecycleMessage: String?
    @State private var vtpmLifecycleError: String?
    @State private var confirmVTPMRestore = false
    @State private var confirmVTPMReset = false

    init(config: HvfEngineConfig = HvfEngineView.defaultConfig()) {
        _session = StateObject(wrappedValue: HvfEngineSession(config: config))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                readinessCard
                if session.config.vtpmStateDir != nil { vtpmLifecycleCard }
                configCard
                statusCard
                screenshotCard
                eventFeedCard
            }
            .padding(20)
        }
        .navigationTitle("HVF Engine")
        .onAppear {
            loadStateFromSession()
            session.attachToRunningVM()
        }
        .confirmationDialog(
            "이 상태에 복구 키를 연결하시겠습니까?",
            isPresented: $confirmVTPMRestore,
            titleVisibility: .visible
        ) {
            Button("검증 후 복원") { restoreVTPMRecovery() }
            Button("취소", role: .cancel) {}
        } message: {
            Text("패키지의 VM ID와 현재 암호화 상태 지문이 정확히 일치할 때만 Keychain 키를 교체합니다.")
        }
        .confirmationDialog(
            "TPM ID를 재설정하시겠습니까?",
            isPresented: $confirmVTPMReset,
            titleVisibility: .visible
        ) {
            Button("기존 상태를 보관하고 재설정", role: .destructive) { resetVTPMIdentity() }
            Button("취소", role: .cancel) {}
        } message: {
            Text("Windows의 BitLocker 및 PCR 봉인 비밀에는 복구 키가 필요할 수 있습니다. BridgeVM은 기존 암호화 상태와 장치 로컬 키를 보관한 뒤 새 TPM으로 시작합니다.")
        }
        .alert(
            "vTPM 수명주기 작업 실패",
            isPresented: Binding(
                get: { vtpmLifecycleError != nil },
                set: { if !$0 { vtpmLifecycleError = nil } }
            )
        ) {
            Button("확인") { vtpmLifecycleError = nil }
        } message: {
            Text(vtpmLifecycleError ?? "알 수 없는 오류")
        }
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text("HVF Engine (Experimental)").font(.largeTitle.bold())
                Text("Windows 11 ARM64 from-scratch HVF backend").foregroundColor(.secondary)
            }
            Spacer()
            statusPill
        }
    }

    private var configCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                pathRow("Target disk", text: $targetDiskPath, chooseDirectory: false)
                pathRow("UEFI vars", text: $uefiVarsPath, chooseDirectory: false)
                pathRow("Evidence dir", text: $evidenceDir, chooseDirectory: true)
                pathRow("CTL file", text: $ctlFilePath, chooseDirectory: false)
                HStack {
                    Text("Watchdog").frame(width: 92, alignment: .leading)
                    Toggle("Enabled", isOn: $watchdogEnabled)
                        .toggleStyle(.checkbox)
                    Stepper("\(watchdogMs) ms", value: $watchdogMs, in: 60_000...86_400_000, step: 30_000)
                        .font(.body.monospaced())
                        .disabled(!watchdogEnabled)
                    Spacer()
                }
                HStack(spacing: 24) {
                    Stepper("RAM \(ramMiB) MiB", value: $ramMiB, in: 1024...65_536, step: 1024)
                        .font(.body.monospaced())
                    Stepper("CPU \(smpCpus)", value: $smpCpus, in: 1...123)
                        .font(.body.monospaced())
                    Spacer()
                }
                HStack(spacing: 18) {
                    Toggle("Clipboard sync", isOn: $clipboardSync)
                    Toggle("Virtio net", isOn: $virtioNet)
                    Toggle("VirGL 3D", isOn: $virtioGpu3d)
                    Toggle("Buffered NVMe", isOn: $nvmeBufferedIO)
                    Toggle("Shared folder", isOn: $shareEnabled)
                    Spacer()
                }
                if shareEnabled {
                    pathRow("Host share", text: $shareHostDir, chooseDirectory: true)
                    HStack {
                        Text("Guest share").frame(width: 92, alignment: .leading)
                        TextField("C:\\bridgevm-share", text: $shareGuestDir)
                            .textFieldStyle(.roundedBorder)
                            .font(.body.monospaced())
                    }
                }
                HStack(spacing: 8) {
                    TextField("Type text into Windows", text: $keyboardInput, onCommit: sendKeyboardText)
                        .textFieldStyle(.roundedBorder)
                    Button("Type", action: sendKeyboardText)
                    Button("Tab") { session.sendKey("tab") }
                    Button("Enter") { session.sendKey("enter") }
                    Button("Space") { session.sendKey("space") }
                }
                HStack(spacing: 8) {
                    Button("Esc") { session.sendKey("esc") }
                    Button("⌫") { session.sendKey("backspace") }.help("Backspace")
                    Button("⌦") { session.sendKey("delete") }.help("Delete")
                    Divider().frame(height: 18)
                    Button("←") { session.sendKey("left") }
                    Button("↑") { session.sendKey("up") }
                    Button("↓") { session.sendKey("down") }
                    Button("→") { session.sendKey("right") }
                    Button("Home") { session.sendKey("home") }
                    Button("End") { session.sendKey("end") }
                    Spacer()
                    Button("Ctrl-Alt-Delete") { session.sendKey("ctrl+alt+delete") }
                }
            }
            .padding(6)
        } label: {
            Label("Boot Configuration", systemImage: "gearshape.2")
        }
    }

    private var readinessCard: some View {
        let report = currentConfig().readiness(repoRoot: session.repoRoot)
        return GroupBox {
            VStack(alignment: .leading, spacing: 6) {
                Label(
                    report.launchReady ? "부팅 준비 완료" : "부팅 차단 \(report.launchBlockers.count)건",
                    systemImage: report.launchReady ? "checkmark.circle.fill" : "xmark.octagon.fill"
                )
                .foregroundColor(report.launchReady ? .green : .red)
                Text(report.releaseReady
                     ? "제품 출시 게이트도 통과했습니다."
                     : "제품 출시 차단 \(report.releaseBlockers.count)건 — 개발 VM 부팅 가능 여부와 별도입니다.")
                    .font(.caption)
                    .foregroundColor(.secondary)
                ForEach(report.issues.prefix(5)) { issue in
                    Text("[\(issue.scope.rawValue)] \(issue.summary)")
                        .font(.caption)
                        .foregroundColor(issue.scope == .launch ? .red : .orange)
                }
                ForEach(report.productLimitations, id: \.self) { limitation in
                    Text("[v1 limitation] \(limitation)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(6)
        } label: {
            Label("Windows HVF Readiness", systemImage: "checklist")
        }
    }

    private var vtpmLifecycleCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text("같은 Mac에서 VM 번들을 옮길 때는 VM ID를 유지합니다. 다른 Mac으로 옮길 때는 암호화된 복구 패키지와 별도 복구 코드를 함께 사용합니다. 복제본은 원본 TPM을 공유하지 않고 재설정해야 합니다.")
                    .font(.caption)
                    .foregroundColor(.secondary)
                HStack(spacing: 8) {
                    Button("복구 패키지 내보내기", action: exportVTPMRecovery)
                    Button("복구 패키지 선택", action: chooseVTPMRecoveryPackage)
                    Button("검증 후 복원") { confirmVTPMRestore = true }
                        .disabled(vtpmRecoveryPackagePath.isEmpty || vtpmRecoveryCodeInput.isEmpty)
                    Spacer()
                    Button("TPM ID 재설정", role: .destructive) { confirmVTPMReset = true }
                }
                .disabled(!vtpmLifecycleAvailable)
                if !vtpmRecoveryPackagePath.isEmpty {
                    Text(vtpmRecoveryPackagePath)
                        .font(.caption2.monospaced())
                        .textSelection(.enabled)
                }
                SecureField("복구 코드", text: $vtpmRecoveryCodeInput)
                    .textFieldStyle(.roundedBorder)
                    .font(.body.monospaced())
                if !vtpmRecoveryCode.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("복구 코드 — 패키지와 분리해 안전하게 보관하세요")
                            .font(.caption.bold())
                        Text(vtpmRecoveryCode)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                    }
                }
                if let vtpmLifecycleMessage {
                    Text(vtpmLifecycleMessage)
                        .font(.caption)
                        .foregroundColor(.green)
                        .textSelection(.enabled)
                }
                if !vtpmLifecycleAvailable {
                    Text("vTPM 상태 작업 전 VM을 완전히 정지하세요.")
                        .font(.caption)
                        .foregroundColor(.orange)
                }
            }
            .padding(6)
        } label: {
            Label("vTPM Identity & Recovery", systemImage: "key.horizontal")
        }
    }

    private var statusCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    Button(action: start) { Label("Start", systemImage: "play.fill") }
                        .disabled(!bootConfigReady)
                    Button(action: session.stop) { Label("Stop", systemImage: "stop.fill") }
                    Button(action: sendCtl) { Label("Send", systemImage: "paperplane.fill") }
                        .disabled(ctlInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    TextField("CLIPGET, CLIPSET ..., or guest shell command", text: $ctlInput, onCommit: sendCtl)
                        .textFieldStyle(.roundedBorder)
                        .font(.body.monospaced())
                }
                HStack(spacing: 24) {
                    infoItem("State", stateText)
                    infoItem("Heartbeat", heartbeatText)
                    infoItem("Events", "\(session.events.count)")
                }
            }
            .padding(6)
        } label: {
            Label("Control Channel", systemImage: "terminal")
        }
    }

    private var screenshotCard: some View {
        GroupBox {
            #if canImport(AppKit)
            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("게스트 화면은 별도 창에서 열립니다.")
                    Text("연결 상태: \(displayConnectionStateText)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                Spacer()
                Button {
                    HvfDisplayWindowController.present(session: session, title: displayWindowTitle)
                } label: {
                    Label("화면 창 열기", systemImage: "macwindow")
                }
            }
            .padding(6)
            #else
            Text("스크린샷에는 AppKit이 필요합니다.").foregroundColor(.secondary)
            #endif
        } label: {
            Label("라이브 디스플레이", systemImage: "display")
        }
    }

    private var eventFeedCard: some View {
        GroupBox {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(session.events.enumerated()), id: \.offset) { _, event in
                        Text(event.displayText)
                            .font(.system(size: 11, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                    }
                }
                .padding(8)
            }
            .frame(minHeight: 220)
            .background(Color(white: 0.1))
            .foregroundColor(Color(white: 0.9))
            .cornerRadius(6)
        } label: {
            Label("BVAGENT Event Feed", systemImage: "list.bullet.rectangle")
        }
    }

    private var statusPill: some View {
        Text(stateText)
            .font(.callout)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(stateColor.opacity(0.18))
            .foregroundColor(stateColor)
            .cornerRadius(8)
    }

    private var stateText: String {
        switch session.connectionState {
        case .stopped: return "Stopped"
        case .booting: return "Booting"
        case let .connected(host): return "Connected: \(host)"
        case .stopping: return "Stopping"
        case .timedOut: return "Timed out"
        }
    }

    private var stateColor: Color {
        switch session.connectionState {
        case .stopped: return .secondary
        case .booting: return .orange
        case .connected: return .green
        case .stopping: return .orange
        case .timedOut: return .red
        }
    }

    private var heartbeatText: String {
        guard let age = session.lastHeartbeatAge else { return "-" }
        return String(format: "%.1fs ago", age)
    }

    private var displayWindowTitle: String {
        let path = session.config.targetDiskPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !path.isEmpty else { return "Windows HVF" }
        let directoryName = URL(fileURLWithPath: path).deletingLastPathComponent().lastPathComponent
        return directoryName.isEmpty ? "Windows HVF" : directoryName
    }

    private var displayConnectionStateText: String {
        switch session.connectionState {
        case .stopped: return "중지됨"
        case .booting: return "부팅 중"
        case let .connected(host): return "연결됨: \(host)"
        case .stopping: return "종료 중"
        case .timedOut: return "시간 초과"
        }
    }

    private var bootConfigReady: Bool {
        currentConfig().readiness(repoRoot: session.repoRoot).launchReady
    }

    private var vtpmLifecycleAvailable: Bool {
        session.connectionState == .stopped || session.connectionState == .timedOut
    }

    private func pathRow(_ label: String, text: Binding<String>, chooseDirectory: Bool) -> some View {
        HStack {
            Text(label).frame(width: 92, alignment: .leading)
            TextField(label, text: text)
                .textFieldStyle(.roundedBorder)
                .font(.body.monospaced())
            Button { choosePath(text: text, directory: chooseDirectory) } label: {
                Image(systemName: "folder")
            }
            .help("Choose \(label)")
        }
    }

    private func infoItem(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundColor(.secondary)
            Text(value).font(.body.monospaced())
        }
    }

    private func start() {
        session.config = currentConfig()
        session.start()
        #if canImport(AppKit)
        HvfDisplayWindowController.present(session: session, title: displayWindowTitle)
        #endif
    }

    private func sendCtl() {
        if session.sendCtl(ctlInput) {
            ctlInput = ""
        }
    }

    private func sendKeyboardText() {
        session.sendText(keyboardInput)
        keyboardInput = ""
    }

    private func currentConfig() -> HvfEngineConfig {
        HvfEngineConfig(targetDiskPath: targetDiskPath,
                        uefiVarsPath: uefiVarsPath,
                        evidenceDir: evidenceDir,
                        watchdogMs: watchdogEnabled ? watchdogMs : nil,
                        ramMiB: ramMiB,
                        smpCpus: smpCpus,
                        clipboardSync: clipboardSync,
                        shareHostDir: shareEnabled ? shareHostDir : nil,
                        shareGuestDir: shareEnabled ? shareGuestDir : nil,
                        virtioNet: virtioNet,
                        virtioGpu3d: virtioGpu3d,
                        nvmeBufferedIO: nvmeBufferedIO,
                        ctlFilePath: ctlFilePath,
                        vtpmStateDir: session.config.vtpmStateDir,
                        swtpmBin: session.config.swtpmBin,
                        vtpmKeyID: session.config.vtpmKeyID)
    }

    private func loadStateFromSession() {
        let cfg = session.config
        targetDiskPath = cfg.targetDiskPath
        uefiVarsPath = cfg.uefiVarsPath
        evidenceDir = cfg.evidenceDir
        watchdogEnabled = cfg.watchdogMs != nil
        watchdogMs = cfg.watchdogMs ?? 900_000
        ramMiB = cfg.ramMiB
        smpCpus = cfg.smpCpus
        clipboardSync = cfg.clipboardSync
        shareEnabled = cfg.shareHostDir != nil && cfg.shareGuestDir != nil
        shareHostDir = cfg.shareHostDir ?? ""
        shareGuestDir = cfg.shareGuestDir ?? "C:\\bridgevm-share"
        virtioNet = cfg.virtioNet
        virtioGpu3d = cfg.virtioGpu3d
        nvmeBufferedIO = cfg.nvmeBufferedIO
        ctlFilePath = cfg.ctlFilePath
    }

    private func choosePath(text: Binding<String>, directory: Bool) {
        #if canImport(AppKit)
        let panel = NSOpenPanel()
        panel.canChooseFiles = !directory
        panel.canChooseDirectories = directory
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            text.wrappedValue = url.path
        }
        #endif
    }

    private func exportVTPMRecovery() {
        #if canImport(AppKit)
        guard vtpmLifecycleAvailable,
              let keyID = session.config.vtpmKeyID,
              let statePath = session.config.vtpmStateDir else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(keyID).bridgevm-vtpm-recovery.json"
        panel.allowedContentTypes = [.json]
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        do {
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            let result = try lifecycle.exportRecovery(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true),
                destination: destination
            )
            vtpmRecoveryCode = result.recoveryCode
            vtpmLifecycleMessage = "복구 패키지를 저장했습니다. 상태 지문: \(result.stateFingerprint)"
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
        #endif
    }

    private func chooseVTPMRecoveryPackage() {
        #if canImport(AppKit)
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.json]
        if panel.runModal() == .OK, let url = panel.url {
            vtpmRecoveryPackagePath = url.path
            vtpmLifecycleMessage = nil
        }
        #endif
    }

    private func restoreVTPMRecovery() {
        guard vtpmLifecycleAvailable,
              let keyID = session.config.vtpmKeyID,
              let statePath = session.config.vtpmStateDir else { return }
        do {
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            try lifecycle.restoreRecovery(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true),
                packageURL: URL(fileURLWithPath: vtpmRecoveryPackagePath),
                recoveryCode: vtpmRecoveryCodeInput
            )
            vtpmRecoveryCodeInput = ""
            vtpmLifecycleMessage = "VM ID와 상태 지문을 검증하고 vTPM 키를 Keychain에 복원했습니다."
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
    }

    private func resetVTPMIdentity() {
        guard vtpmLifecycleAvailable,
              let keyID = session.config.vtpmKeyID,
              let statePath = session.config.vtpmStateDir else { return }
        do {
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            let result = try lifecycle.resetIdentity(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true)
            )
            vtpmLifecycleMessage = result.archivedStatePath.map {
                "새 TPM ID로 전환했습니다. 이전 상태: \($0) · 영수증: \(result.receiptPath)"
            } ?? "새 TPM ID로 전환했습니다. 영수증: \(result.receiptPath)"
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
    }

    static func defaultConfig() -> HvfEngineConfig {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let evidence = home.appendingPathComponent("BridgeVM/HVF/evidence", isDirectory: true).path
        return HvfEngineConfig(targetDiskPath: "",
                               uefiVarsPath: "",
                               evidenceDir: evidence,
                               watchdogMs: nil,
                               ramMiB: 6144,
                               smpCpus: 4,
                               clipboardSync: true,
                               shareHostDir: nil,
                               shareGuestDir: nil,
                               virtioNet: false,
                               virtioGpu3d: true,
                               nvmeBufferedIO: true,
                               ctlFilePath: "\(evidence)/bvagent.ctl")
    }
}

enum HvfHostKeyCommand: Equatable {
    case key(String)
    case text(String)
    case ignored

    static func resolve(characters: String, modifiers: EventModifiers = []) -> HvfHostKeyCommand {
        if characters == "\u{7f}", modifiers.contains(.control), modifiers.contains(.option) {
            return .key("ctrl+alt+delete")
        }
        switch characters {
        case "\u{1b}": return .key("esc")
        case "\u{7f}": return .key("backspace")
        case "\u{f728}": return .key("delete")
        case "\u{f700}": return .key("up")
        case "\u{f701}": return .key("down")
        case "\u{f702}": return .key("left")
        case "\u{f703}": return .key("right")
        case "\u{f729}": return .key("home")
        case "\u{f72b}": return .key("end")
        case "\u{f72c}": return .key("pageup")
        case "\u{f72d}": return .key("pagedown")
        case "\t": return .key("tab")
        case "\r", "\n": return .key("enter")
        default:
            guard !characters.isEmpty,
                  !modifiers.contains(.command),
                  !modifiers.contains(.control),
                  !modifiers.contains(.option) else { return .ignored }
            return .text(characters)
        }
    }
}
