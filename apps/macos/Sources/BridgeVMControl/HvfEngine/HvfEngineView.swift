import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif
struct HvfEngineView: View {
    @ObservedObject private(set) var session: HvfEngineSession
    @StateObject private var readiness = HvfRuntimeReadinessModel()
    @State private var startRefusal: String?
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
    @State private var nvmeBufferedIO = false
    @State private var ctlFilePath = ""
    @State private var ctlInput = ""
    @State private var keyboardInput = ""
    @State var vtpmRecoveryCode = ""
    @State var vtpmRecoveryPackagePath = ""
    @State var vtpmRecoveryCodeInput = ""
    @State var vtpmLifecycleMessage: String?
    @State var vtpmLifecycleError: String?
    @State var confirmVTPMRestore = false
    @State var confirmVTPMReset = false
    @State var vtpmMutationReservation: HvfRuntimeMutationReservation?
    init(session: HvfEngineSession) {
        _session = ObservedObject(wrappedValue: session)
    }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                statusCard
                screenshotCard
                readinessCard
                if session.config.vtpmStateDir != nil { vtpmLifecycleCard }
                configCard
                HvfRuntimeDiagnosticsSettings(
                    evidenceDir: $evidenceDir, ctlFilePath: $ctlFilePath,
                    watchdogEnabled: $watchdogEnabled, watchdogMs: $watchdogMs,
                    nvmeBufferedIO: $nvmeBufferedIO, ctlInput: $ctlInput, sendCtl: sendCtl
                ) { label, text, chooseDirectory in
                    pathRow(label, text: text, chooseDirectory: chooseDirectory)
                }
                HvfWindowsSnapshotCard(config: currentConfig(), repoRoot: session.repoRoot, session: session)
                eventFeedCard
            }
            .padding(20)
        }
        .navigationTitle("HVF Engine")
        .accessibilityIdentifier("bridgevm.windows.runtime.view")
        .onChange(of: confirmVTPMRestore) { _, shown in if !shown { releaseVTPMReservation() } }
        .onChange(of: confirmVTPMReset) { _, shown in if !shown { releaseVTPMReservation() } }
        .onDisappear { releaseVTPMReservation() }
        .onAppear {
            loadStateFromSession()
            readiness.request(configuration: currentConfig(), repoRoot: session.repoRoot)
            session.attachIfStopped()
        }
        .onChange(of: session.hasActiveRuntimeWork) { _, active in
            if active { startRefusal = nil }
        }
        .onChange(of: currentConfig()) { _, configuration in
            readiness.request(configuration: configuration, repoRoot: session.repoRoot)
        }
        .onChange(of: session.repoRoot) { _, repoRoot in
            readiness.request(configuration: currentConfig(), repoRoot: repoRoot)
        }
        .confirmationDialog(
            "이 상태에 복구 키를 연결하시겠습니까?",
            isPresented: $confirmVTPMRestore,
            titleVisibility: .visible
        ) {
            Button("검증 후 복원") { restoreVTPMRecovery() }
            Button("취소", role: .cancel) { releaseVTPMReservation() }
        } message: {
            Text("패키지의 VM ID와 현재 암호화 상태 지문이 정확히 일치할 때만 Keychain 키를 교체합니다.")
        }
        .confirmationDialog(
            "TPM ID를 재설정하시겠습니까?",
            isPresented: $confirmVTPMReset,
            titleVisibility: .visible
        ) {
            Button("기존 상태를 보관하고 재설정", role: .destructive) { resetVTPMIdentity() }
            Button("취소", role: .cancel) { releaseVTPMReservation() }
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
        HvfRuntimeHeader(title: session.config.libraryContext?.config.name ?? displayWindowTitle, state: stateText, stateColor: stateColor,
                         ramMiB: session.config.ramMiB, cpus: session.config.smpCpus)
    }
    private var configCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                pathRow("Target disk", text: $targetDiskPath, chooseDirectory: false)
                pathRow("UEFI vars", text: $uefiVarsPath, chooseDirectory: false)
                HStack(spacing: 24) {
                    Stepper("RAM \(ramMiB) MiB", value: $ramMiB, in: 1024...65_536, step: 1024)
                        .font(.body.monospaced())
                    Stepper("CPU \(smpCpus)", value: $smpCpus, in: HvfEngineConfig.supportedCPURange)
                        .font(.body.monospaced())
                    Spacer()
                }
                HStack(spacing: 18) {
                    Toggle("Clipboard sync", isOn: $clipboardSync).accessibilityIdentifier("bridgevm.runtime.clipboard")
                    Toggle("Virtio net", isOn: $virtioNet).accessibilityIdentifier("bridgevm.runtime.network")
                    if session.config.allowsExperimental3D {
                        Toggle("Graphics Lab 3D", isOn: $virtioGpu3d)
                    }
                    Toggle("Shared folder", isOn: $shareEnabled).accessibilityIdentifier("bridgevm.runtime.share.enabled")
                    Spacer()
                }
                if shareEnabled {
                    pathRow("Host share", text: $shareHostDir, chooseDirectory: true).accessibilityIdentifier("bridgevm.runtime.share.host")
                    HStack {
                        Text("Guest share").frame(width: 92, alignment: .leading)
                        TextField("C:\\bridgevm-share", text: $shareGuestDir)
                            .textFieldStyle(.roundedBorder)
                            .font(.body.monospaced()).accessibilityIdentifier("bridgevm.runtime.share.guest")
                    }
                }
                HvfRuntimeKeyboardInput(session: session, draft: $keyboardInput)
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
        HvfRuntimeReadinessView(model: readiness, configuration: currentConfig(), repoRoot: session.repoRoot)
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
                    Button("검증 후 복원") { beginVTPMConfirmation(.vtpmRecoveryRestore) }
                        .disabled(vtpmRecoveryPackagePath.isEmpty || vtpmRecoveryCodeInput.isEmpty)
                    Spacer()
                    Button("TPM ID 재설정", role: .destructive) { beginVTPMConfirmation(.vtpmReset) }
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
        HvfRuntimeStatusCard(session: session, ready: bootConfigReady, stateText: stateText,
            heartbeatText: heartbeatText, refusal: startRefusal, start: start)
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
                }.accessibilityIdentifier("bridgevm.runtime.display.open")
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

    private var pendingWorkText: String? { session.runtimePendingWorkText }
    private var stateText: String {
        if let pendingWorkText { return pendingWorkText }
        switch session.connectionState {
        case .stopped: return "Stopped"
        case .booting: return "Booting"
        case let .connected(host): return "Connected: \(host)"
        case .stopping: return "Stopping"
        case .timedOut: return "Timed out"
        }
    }

    private var stateColor: Color {
        if pendingWorkText != nil { return .orange }
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
        guard !path.isEmpty else { return session.config.libraryContext?.config.name ?? "Windows HVF" }
        let directoryName = URL(fileURLWithPath: path).deletingLastPathComponent().lastPathComponent
        return session.config.libraryContext?.config.name ?? (directoryName.isEmpty ? "Windows HVF" : directoryName)
    }

    private var displayConnectionStateText: String {
        if let pendingWorkText { return pendingWorkText }
        switch session.connectionState {
        case .stopped: return "중지됨"
        case .booting: return "부팅 중"
        case let .connected(host): return "연결됨: \(host)"
        case .stopping: return "종료 중"
        case .timedOut: return "시간 초과"
        }
    }

    private var bootConfigReady: Bool {
        readiness.report(configuration: currentConfig(), repoRoot: session.repoRoot)?.launchReady == true
    }

    var vtpmLifecycleAvailable: Bool { !session.hasActiveRuntimeWork }

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

    private func start() {
        startRefusal = nil
        if case let .refused(reason) = session.requestGUIStart(configuration: currentConfig()) {
            startRefusal = reason; return
        }
        #if canImport(AppKit)
        HvfDisplayWindowController.present(session: session, title: displayWindowTitle)
        #endif
    }

    private func sendCtl() {
        if session.sendCtl(ctlInput) {
            ctlInput = ""
        }
    }

    private func currentConfig() -> HvfEngineConfig {
        var config = session.config
        config.targetDiskPath = targetDiskPath
        config.uefiVarsPath = uefiVarsPath
        config.evidenceDir = evidenceDir
        config.watchdogMs = watchdogEnabled ? watchdogMs : nil
        config.ramMiB = ramMiB
        config.smpCpus = smpCpus
        config.clipboardSync = clipboardSync
        config.shareHostDir = shareEnabled ? shareHostDir : nil
        config.shareGuestDir = shareEnabled ? shareGuestDir : nil
        config.virtioNet = virtioNet
        config.virtioGpu3d = virtioGpu3d
        config.nvmeBufferedIO = nvmeBufferedIO
        config.ctlFilePath = ctlFilePath
        return config
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
                               nvmeBufferedIO: false,
                               ctlFilePath: "\(evidence)/bvagent.ctl")
    }
}
