import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif
struct HvfEngineView: View {
    @ObservedObject private(set) var session: HvfEngineSession
    @StateObject private var readiness = HvfRuntimeReadinessModel()
    @State private var startRefusal: String?
    @State private var showingSettings = false
    @State private var targetDiskPath = ""
    @State private var uefiVarsPath = ""
    @State private var evidenceDir = ""
    @State private var watchdogEnabled = false
    @State private var watchdogMs = 900_000
    @State private var virtioGpu3d = false
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
    var content: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                statusCard
                screenshotCard
                keyboardCard
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
                eventFeedCard
            }
            .padding(20)
        }
    }

    var body: some View {
        content
        .navigationTitle("HVF Engine")
        .focusedSceneValue(\.hvfVMSettings, { showingSettings = true })
        .sheet(isPresented: $showingSettings) { HvfVMSettingsSheet(session: session) }
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
            "Restore the recovery key for this TPM state?",
            isPresented: $confirmVTPMRestore,
            titleVisibility: .visible
        ) {
            Button("Verify and Restore") { restoreVTPMRecovery() }
            Button("Cancel", role: .cancel) { releaseVTPMReservation() }
        } message: {
            Text("BridgeVM replaces the key in Keychain only when the package matches this VM’s ID and the fingerprint of its encrypted TPM state.")
        }
        .confirmationDialog(
            "Reset the TPM identity?",
            isPresented: $confirmVTPMReset,
            titleVisibility: .visible
        ) {
            Button("Archive State and Reset", role: .destructive) { resetVTPMIdentity() }
            Button("Cancel", role: .cancel) { releaseVTPMReservation() }
        } message: {
            Text("Windows may ask for a BitLocker recovery key, and secrets sealed to TPM measurements may require recovery. BridgeVM archives the encrypted state and device-local key before starting with a new TPM.")
        }
        .alert(
            "vTPM operation failed",
            isPresented: Binding(
                get: { vtpmLifecycleError != nil },
                set: { if !$0 { vtpmLifecycleError = nil } }
            )
        ) {
            Button("OK") { vtpmLifecycleError = nil }
        } message: {
            Text(vtpmLifecycleError ?? "Unknown error")
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
                pathRow("UEFI variables", text: $uefiVarsPath, chooseDirectory: false)
                if session.config.allowsExperimental3D {
                    Toggle("Graphics Lab 3D", isOn: $virtioGpu3d)
                }
            }
            .padding(6)
        } label: {
            Label("Boot configuration", systemImage: "gearshape.2")
        }
    }

    private var keyboardCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text("Click where you want to type in the Windows display first.")
                    .font(.callout).foregroundStyle(.secondary)
                HvfRuntimeKeyboardInput(session: session, draft: $keyboardInput)
                HvfRuntimeSpecialKeys(session: session)
            }
            .padding(6)
        } label: {
            Label("Windows keyboard", systemImage: "keyboard")
        }
    }

    private var readinessCard: some View {
        HvfRuntimeReadinessView(model: readiness, configuration: currentConfig(), repoRoot: session.repoRoot)
    }

    private var vtpmLifecycleCard: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text("Keep the VM ID when moving a VM on this Mac. To move to another Mac, use the encrypted recovery package and its separate code. Reset the TPM identity for a duplicate VM.")
                    .font(.caption)
                    .foregroundColor(.secondary)
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) { vtpmRecoveryActions }
                    VStack(alignment: .leading, spacing: 8) { vtpmRecoveryActions }
                }
                .disabled(!vtpmLifecycleAvailable)
                if !vtpmRecoveryPackagePath.isEmpty {
                    Text(vtpmRecoveryPackagePath)
                        .font(.caption2.monospaced())
                        .textSelection(.enabled)
                }
                SecureField("Recovery code", text: $vtpmRecoveryCodeInput)
                    .textFieldStyle(.roundedBorder)
                    .font(.body.monospaced())
                if !vtpmRecoveryCode.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Store this recovery code separately from the package.")
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
                    Text("Shut down the VM before changing its vTPM state.")
                        .font(.caption)
                        .foregroundColor(.orange)
                }
            }
            .padding(6)
        } label: {
            Label("vTPM identity and recovery", systemImage: "key.horizontal")
        }
    }

    @ViewBuilder private var vtpmRecoveryActions: some View {
        HStack(spacing: 8) {
            Button("Export Package…", action: exportVTPMRecovery)
            Button("Choose Package…", action: chooseVTPMRecoveryPackage)
        }
        HStack(spacing: 8) {
            Button("Verify and Restore") { beginVTPMConfirmation(.vtpmRecoveryRestore) }
                .disabled(vtpmRecoveryPackagePath.isEmpty || vtpmRecoveryCodeInput.isEmpty)
            Spacer(minLength: 0)
            Button("Reset TPM Identity…", role: .destructive) { beginVTPMConfirmation(.vtpmReset) }
        }
    }

    private var statusCard: some View {
        HvfRuntimeStatusCard(session: session, ready: bootConfigReady, stateText: stateText,
            stateCode: HvfRuntimeAccessibilityState.code(pending: pendingWorkText != nil, connection: session.connectionState),
            refusal: startRefusal, start: start)
    }

    private var screenshotCard: some View {
        GroupBox {
            #if canImport(AppKit)
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("The Windows display opens in a separate window.")
                        Text("Connection: \(displayConnectionStateText)")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                    Spacer()
                    Button {
                        HvfDisplayWindowController.present(session: session, title: displayWindowTitle)
                    } label: {
                        Label("Open Display", systemImage: "macwindow")
                    }.accessibilityIdentifier("bridgevm.runtime.display.open")
                }
            }
            .padding(6)
            #else
            Text("Screenshots require AppKit.").foregroundColor(.secondary)
            #endif
        } label: {
            Label("Live display", systemImage: "display")
        }
    }

    private var eventFeedCard: some View {
        GroupBox {
            ScrollView {
                // One bounded selectable text in a fixed-height pane: per-event responders and an
                // ideally-sized pane measuring the whole feed each stall the main thread for seconds.
                Text(HvfEventFeedText.render(session.events))
                    .font(.system(size: 11, design: .monospaced))
                    .lineSpacing(4)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
                .padding(8)
            }
            .frame(height: 220)
            .background(Color(white: 0.1))
            .foregroundColor(Color(white: 0.9))
            .cornerRadius(6)
        } label: {
            Label("Guest agent events", systemImage: "list.bullet.rectangle")
        }
    }

    private var pendingWorkText: String? { session.runtimePendingWorkText }
    private var stateText: String { displayConnectionStateText }

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

    private var displayWindowTitle: String {
        HvfDisplayWindowTitle.resolve(libraryName: session.config.libraryContext?.config.name,
                                      targetDiskPath: session.config.targetDiskPath)
    }

    private var displayConnectionStateText: String {
        if let pendingWorkText { return pendingWorkText }
        switch session.connectionState {
        case .stopped: return "Stopped"
        case .booting: return "Booting Windows"
        case let .connected(host): return "Connected to \(host)"
        case .stopping: return "Shutting down"
        case .timedOut: return "Connection timed out"
        }
    }

    private var bootConfigReady: Bool {
        readiness.report(configuration: currentConfig(), repoRoot: session.repoRoot)?.launchReady == true
    }

    var vtpmLifecycleAvailable: Bool { !session.hasActiveRuntimeWork }

    private func pathRow(_ label: String, text: Binding<String>, chooseDirectory: Bool, identifier: String = "", chooseIdentifier: String = "") -> some View {
        HStack {
            Text(label).frame(width: 92, alignment: .leading)
            TextField("", text: text, prompt: Text("\(label) path"))
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .font(.body.monospaced())
                .accessibilityIdentifier(identifier)
            Button { choosePath(text: text, directory: chooseDirectory) } label: { Image(systemName: "folder") }
            .accessibilityIdentifier(chooseIdentifier)
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
        virtioGpu3d = cfg.virtioGpu3d
        nvmeBufferedIO = cfg.nvmeBufferedIO
        ctlFilePath = cfg.ctlFilePath
    }

    private func choosePath(text: Binding<String>, directory: Bool) {
        #if canImport(AppKit)
        FileSelection.choose(directories: directory) { url in
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
                               virtioGpu3d: false,
                               nvmeBufferedIO: false,
                               ctlFilePath: "\(evidence)/bvagent.ctl", allowsExperimental3D: true)
    }
}

#if canImport(AppKit)
#endif

extension HvfEngineView {
    func beginVTPMConfirmation(_ kind: HvfRuntimeMutationKind) {
        guard let reservation = session.reserveRuntimeMutation(kind: kind) else { return }
        vtpmMutationReservation = reservation
        if kind == .vtpmRecoveryRestore { confirmVTPMRestore = true }
        else { confirmVTPMReset = true }
    }
    func releaseVTPMReservation() {
        guard let reservation = vtpmMutationReservation else { return }
        vtpmMutationReservation = nil
        session.finishRuntimeMutation(reservation)
    }
    func exportVTPMRecovery() {
        #if canImport(AppKit)
        guard let reservation = session.reserveRuntimeMutation(kind: .vtpmRecoveryExport) else { return }
        defer { session.finishRuntimeMutation(reservation) }
        guard let keyID = reservation.configuration.vtpmKeyID,
              let statePath = reservation.configuration.vtpmStateDir else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(keyID).bridgevm-vtpm-recovery.json"
        panel.allowedContentTypes = [.json]
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        guard session.validateRuntimeMutation(reservation) else { return }
        do {
            try reservation.checkEffect()
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            let result = try lifecycle.exportRecovery(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true),
                destination: destination
            )
            vtpmRecoveryCode = result.recoveryCode
            vtpmLifecycleMessage = "Saved the recovery package. State fingerprint: \(result.stateFingerprint)"
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
        #endif
    }

    func chooseVTPMRecoveryPackage() {
        #if canImport(AppKit)
        guard let reservation = session.reserveRuntimeMutation(kind: .vtpmRecoveryRestore) else { return }
        defer { session.finishRuntimeMutation(reservation) }
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

    func restoreVTPMRecovery() {
        guard let reservation = vtpmMutationReservation, reservation.kind == .vtpmRecoveryRestore else { return }
        vtpmMutationReservation = nil // The executing operation now owns the reservation through return.
        defer { session.finishRuntimeMutation(reservation) }
        guard session.validateRuntimeMutation(reservation),
              let keyID = reservation.configuration.vtpmKeyID,
              let statePath = reservation.configuration.vtpmStateDir else { return }
        do {
            try reservation.checkEffect()
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            try lifecycle.restoreRecovery(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true),
                packageURL: URL(fileURLWithPath: vtpmRecoveryPackagePath),
                recoveryCode: vtpmRecoveryCodeInput
            )
            vtpmRecoveryCodeInput = ""
            vtpmLifecycleMessage = "Verified the VM ID and state fingerprint and restored the vTPM key to Keychain."
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
    }

    func resetVTPMIdentity() {
        guard let reservation = vtpmMutationReservation, reservation.kind == .vtpmReset else { return }
        vtpmMutationReservation = nil // The executing operation now owns the reservation through return.
        defer { session.finishRuntimeMutation(reservation) }
        guard session.validateRuntimeMutation(reservation),
              let keyID = reservation.configuration.vtpmKeyID,
              let statePath = reservation.configuration.vtpmStateDir else { return }
        do {
            try reservation.checkEffect()
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            let result = try lifecycle.resetIdentity(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true)
            )
            vtpmLifecycleMessage = result.archivedStatePath.map {
                "Reset the TPM identity. Archived state: \($0). Receipt: \(result.receiptPath)"
            } ?? "Reset the TPM identity. Receipt: \(result.receiptPath)"
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
    }

}
