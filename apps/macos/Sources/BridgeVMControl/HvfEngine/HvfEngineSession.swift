import Foundation
import Combine
#if os(macOS)
import Darwin
#endif
#if canImport(AppKit)
import AppKit
#endif

@MainActor
final class HvfEngineSession: ObservableObject {
    @Published var config: HvfEngineConfig
    @Published var connectionState: HvfConnectionState = .stopped
    @Published var lastHeartbeatAge: TimeInterval?
    @Published var events: [BvAgentEvent] = []
    var repoRoot: URL
    var workAdmission: LibraryWorkAdmission?
    private var process: Process?
    private var timer: Timer?
    private var tailReader = TailOffsetReader()
    private var lastHeartbeatDate: Date?
    private var serviceStarted = false
    private var pendingPaste: HvfClipboardPaste?
    private let inputDriver = HvfSessionInputDriver()
    private var stopCommandSent = false
    private var stopDeadline: Date?
    private var attachedToExistingProcess = false
    private var nextAttachedLivenessCheck = Date.distantPast
    private var liveInputHandle: FileHandle?
    private var liveInputPath: URL?
    private var liveInputWriteFailureReported = false
    private var processLaunch: (Process) throws -> Void = { try $0.run() }
    private let processIsRunning: (String) -> Bool
    private(set) var ownedProcessIdentity: HvfOwnedRuntimeIdentity?
    private(set) var lastOwnedExit: HvfOwnedRuntimeExit?
    private let vtpmKeyProvider: VTPMStateKeyProviding

    init(
        config: HvfEngineConfig,
        repoRoot: URL = HvfEngineSession.defaultRepoRoot(),
        processIsRunning: @escaping (String) -> Bool = { Shell.isProcessRunning(matching: $0) },
        vtpmKeyProvider: VTPMStateKeyProviding = KeychainVTPMStateKeyStore()
    ) {
        self.config = config
        self.repoRoot = repoRoot
        self.processIsRunning = processIsRunning
        self.vtpmKeyProvider = vtpmKeyProvider
        inputDriver.onPoll = { [weak self] in self?.poll() }
        inputDriver.onDiagnostic = { [weak self] in self?.append(.unknown($0)) }
    }

    // The injected launcher must run the supplied Process or throw before spawning it.
    convenience init(config: HvfEngineConfig, repoRoot: URL,
                     processLaunch: @escaping (Process) throws -> Void,
                     processIsRunning: @escaping (String) -> Bool,
                     vtpmKeyProvider: VTPMStateKeyProviding) {
        self.init(config: config, repoRoot: repoRoot,
                  processIsRunning: processIsRunning, vtpmKeyProvider: vtpmKeyProvider)
        self.processLaunch = processLaunch
    }

    deinit {
        timer?.invalidate()
        process?.terminate()
        try? liveInputHandle?.close()
    }

    @discardableResult
    func start(policy: HvfRuntimeStartPolicy = .attachOrStart) -> HvfRuntimeStartOutcome {
        if let refusal = workAdmission?(true) { return .refused(refusal) }
        guard process?.isRunning != true else {
            append(.unknown("launch ignored: HVF engine is already running"))
            return .refused("The owned runtime is still running")
        }
        if process != nil { markStopped() }
        if policy == .requireNew, attachedToExistingProcess || processIsRunning(config.targetDiskPath) {
            return .refused("An existing runtime cannot be adopted by a require-new launch")
        }
        if policy == .attachOrStart, attachToRunningVM() {
            append(.unknown("attached to the already running HVF engine; duplicate launch prevented"))
            return .observedAttachment
        }
        let readiness = config.readiness(repoRoot: repoRoot)
        guard readiness.launchReady else {
            for blocker in readiness.launchBlockers {
                append(.unknown("launch readiness blocked [\(blocker.code)]: \(blocker.summary)"))
            }
            connectionState = .stopped
            return .failed(.readiness, readiness.launchBlockers.map(\.code).joined(separator: ","))
        }
        timer?.invalidate()
        timer = nil
        process = nil
        closeLiveInput()
        do {
            try HvfRuntimePreparation.prepare(config: config)
        } catch {
            append(.unknown("launch failed: unable to prepare HVF runtime files: \(error.localizedDescription)"))
            connectionState = .stopped
            return .failed(.preparation, error.localizedDescription)
        }
        // R1 product path: the typed runtime (hvf-runner --launch-spec)
        // whenever the packaged runner exists. The wrapper remains the
        // evidence-harness fallback so a source checkout without a release
        // runner build keeps working.
        let runner = repoRoot.appendingPathComponent("target/release/hvf-runner")
        let useTypedRuntime = FileManager.default.isExecutableFile(atPath: runner.path)
        let wrapper = repoRoot.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        if !useTypedRuntime {
            guard FileManager.default.isExecutableFile(atPath: wrapper.path) else {
                append(.unknown("launch failed: installed-boot wrapper not found at \(wrapper.path)"))
                connectionState = .stopped
                return .failed(.helper, "The installed-boot wrapper is unavailable")
            }
        }
        tailReader = TailOffsetReader()
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        stopCommandSent = false
        stopDeadline = nil
        events = []
        liveInputWriteFailureReported = false
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        if useTypedRuntime {
            let firmware = repoRoot.appendingPathComponent("firmware/edk2-aarch64-secure-code.fd")
            let fallbackFirmware = repoRoot.appendingPathComponent("crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd")
            let firmwarePath = FileManager.default.fileExists(atPath: firmware.path)
                ? firmware.path : fallbackFirmware.path
            proc.arguments = config.runnerArguments(
                manifestPath: config.evidenceDir + "/launch-manifest.json",
                runnerPath: runner.path,
                firmwareCodePath: firmwarePath,
                probePath: repoRoot.appendingPathComponent("target/release/examples/hvf_gic_boot_probe").path)
            append(.unknown("launching through the typed runtime (hvf-runner --launch-spec)"))
        } else {
            proc.arguments = config.wrapperArguments()
        }
        proc.currentDirectoryURL = repoRoot
        proc.environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        let vtpmKeyInput: VTPMProcessKeyInput?
        do {
            vtpmKeyInput = try VTPMStateSecurity.processInput(
                for: config,
                provider: vtpmKeyProvider
            )
            vtpmKeyInput?.attach(to: proc)
        } catch {
            append(.unknown("launch failed: unable to unlock encrypted vTPM state: \(error.localizedDescription)"))
            connectionState = .stopped
            return .failed(.keyAccess, error.localizedDescription)
        }
        process = proc
        attachedToExistingProcess = false
        connectionState = .booting
        do {
            try processLaunch(proc)
            let identity = HvfOwnedRuntimeIdentity(token: UUID(), processID: proc.processIdentifier)
            ownedProcessIdentity = identity
            lastOwnedExit = nil
            do {
                try vtpmKeyInput?.deliverAfterLaunch()
            } catch {
                proc.terminate()
                append(.unknown("launch failed: unable to deliver the vTPM state key: \(error.localizedDescription)"))
                connectionState = .stopping
                startPolling()
                return .failed(.keyDelivery, error.localizedDescription)
            }
            beginOwnedInputBoot()
            startPolling()
            return .ownedLaunchAccepted(identity)
        } catch {
            vtpmKeyInput?.discard()
            append(.unknown("launch failed: \(error.localizedDescription)"))
            connectionState = .stopped
            process = nil
            return .failed(.processLaunch, error.localizedDescription)
        }
    }

    @discardableResult
    func stopOwned(expectedToken: UUID) -> HvfRuntimeStopOutcome {
        if ownedProcessIdentity == nil, lastOwnedExit?.identity.token == expectedToken { return .alreadyStopped }
        guard ownedProcessIdentity?.token == expectedToken, process != nil else { return .notOwned }
        return requestStop()
    }

    func stop() { _ = requestStop() }

    private func requestStop() -> HvfRuntimeStopOutcome {
        if connectionState == .stopping { return .alreadyStopping(deadline: stopDeadline) }
        let ownsRunningProcess = process?.isRunning == true
        let attachedProcessIsRunning = attachedToExistingProcess && processIsRunning(config.targetDiskPath)
        guard ownsRunningProcess || attachedProcessIsRunning else {
            markStopped()
            return .alreadyStopped
        }
        connectionState = .stopping
        cancelOrderedInputTarget()
        stopDeadline = Date().addingTimeInterval(180)
        sendGracefulStopIfReady()
        if timer == nil { startPolling() }
        return .requested(deadline: stopDeadline)
    }

    @discardableResult
    func attachToRunningVM(reportRefusal: Bool = true) -> Bool {
        guard workAdmission?(reportRefusal) == nil else { return false }
        guard process?.isRunning != true else { return false }
        guard processIsRunning(config.targetDiskPath) else { return false }
        if process != nil { markStopped() }
        timer?.invalidate()
        timer = nil
        process = nil
        closeLiveInput()
        attachedToExistingProcess = true
        // The guard above just paid for a pgrep; don't repeat it on the first poll.
        nextAttachedLivenessCheck = HvfAttachedLivenessSchedule.next(after: Date())
        resetObservedRuntimeState(clearEvents: true)
        connectionState = .booting
        startPolling()
        return true
    }

    @discardableResult
    func sendCtl(_ line: String) -> Bool {
        let cleaned: String
        switch HvfGuestCommand.normalize(line) {
        case let .success(command):
            cleaned = command
        case let .failure(error):
            append(.unknown("control command rejected: \(error.message)"))
            return false
        }
        return appendControlCommand(cleaned)
    }

    private func appendControlCommand(_ cleaned: String) -> Bool {
        guard serviceStarted else {
            append(.unknown("control command refused: guest service has not started"))
            return false
        }
        let path = config.ctlFilePath
        try? FileManager.default.createDirectory(atPath: (path as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: path) {
            FileManager.default.createFile(atPath: path, contents: nil)
        }
        guard let data = "\(cleaned)\n".data(using: .utf8) else { return false }
        do {
            let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
        } catch {
            append(.unknown("control command write failed: \(error.localizedDescription)"))
            return false
        }
        return true
    }

    func sendKey(_ action: String) {
        guard pendingPaste == nil else { append(.unknown("key input refused: clipboard paste pending")); return }
        if inputDriver.route(.key(action), binding: inputBinding) != .legacy { return }
        appendLiveInput("KEY \(action)")
    }

    @discardableResult
    func sendText(_ value: String) -> HvfTextInputSubmission {
        guard pendingPaste == nil else { append(.unknown("text input refused: clipboard paste pending")); return .refused }
        guard !value.isEmpty else { return .refused }
        switch inputDriver.route(.text(value), binding: inputBinding) {
        case .queued: return .acceptedForProcessing
        case .refused: return .refused
        case .legacy: break
        }
        let plan = HvfTextInputPlan.make(for: value)
        for chunk in plan.hidChunks {
            appendLiveInput("KEY text-hex:\(chunk)")
        }
        if let base64 = plan.clipboardBase64 {
            guard serviceStarted, case .connected = connectionState else {
                append(.unknown("clipboard paste refused: guest service is not connected")); return .refused
            }
            let request = HvfClipboardPaste(base64: base64, now: Date())
            guard sendCtl(request.command) else { return .refused }
            pendingPaste = request
            return .acceptedForProcessing
        }
        return .legacyAttempted
    }

    #if canImport(AppKit)
    func sendPointerClick(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction("click", location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerPress(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction("press", location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerMove(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction("move", location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerRelease(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction("release", location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerRightClick(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction("right-click", location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerRightPress(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction("right-press", location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerScroll(_ delta: Int8, location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        guard delta != 0, let point = mappedPointer(location, viewSize: viewSize, imageSize: imageSize) else { return }
        if inputDriver.route(.pointer("scroll:\(delta)@\(point.x)x\(point.y)"), binding: inputBinding) != .legacy { return }
        appendLiveInput("POINTER scroll:\(delta)@\(point.x)x\(point.y)")
    }

    private func sendPointerAction(_ action: String, location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        guard let point = mappedPointer(location, viewSize: viewSize, imageSize: imageSize) else { return }
        let verb = action == "release" ? "releaseall" : action.replacingOccurrences(of: "-", with: "")
        if inputDriver.route(.pointer("\(verb):\(point.x)x\(point.y)"), binding: inputBinding) != .legacy { return }
        appendLiveInput("POINTER \(action):\(point.x)x\(point.y)")
    }

    private func mappedPointer(_ location: CGPoint, viewSize: CGSize, imageSize: CGSize) -> (x: UInt16, y: UInt16)? {
        guard let point = HvfDisplayCoordinates.absolutePointer(
            location: location,
            viewSize: viewSize,
            imageSize: imageSize
        ) else { return nil }
        return point
    }
    #endif

    private func appendLiveInput(_ line: String) {
        guard inputDriver.allowLegacyWrite(binding: inputBinding) else { return }
        let path = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("input.ctl")
        guard let data = "\(line)\n".data(using: .utf8) else { return }
        do {
            let handle = try liveInputHandle(for: path)
            #if os(macOS)
            guard flock(handle.fileDescriptor, LOCK_EX) == 0 else { return }
            defer { flock(handle.fileDescriptor, LOCK_UN) }
            #endif
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
            liveInputWriteFailureReported = false
        } catch {
            closeLiveInput()
            if !liveInputWriteFailureReported {
                append(.unknown("live input write failed: \(error.localizedDescription)"))
                liveInputWriteFailureReported = true
            }
        }
    }

    private func liveInputHandle(for path: URL) throws -> FileHandle {
        if let liveInputHandle, liveInputPath == path {
            return liveInputHandle
        }
        closeLiveInput()
        if !FileManager.default.fileExists(atPath: path.path) {
            FileManager.default.createFile(atPath: path.path, contents: nil)
        }
        let handle = try FileHandle(forWritingTo: path)
        try handle.seekToEnd()
        liveInputHandle = handle
        liveInputPath = path
        return handle
    }

    private func closeLiveInput() {
        try? liveInputHandle?.close()
        liveInputHandle = nil
        liveInputPath = nil
    }

    private func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
        poll()
    }


    func poll() {
        let logURL = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("run.log")
        let lines = tailReader.readNewLines(from: logURL)
        if let ready = pendingPaste?.consume(lines: lines, now: Date()) {
            pendingPaste = nil
            if ready, case .connected = connectionState { appendLiveInput("KEY ctrl+v") }
            else { append(.unknown("clipboard paste failed, expired or canceled")) }
        }
        if !lines.isEmpty {
            for event in BvAgentEvent.parse(lines: lines) {
                handle(event)
            }
        }
        if let lastHeartbeatDate {
            lastHeartbeatAge = Date().timeIntervalSince(lastHeartbeatDate)
        }
        if let process, !process.isRunning {
            markStopped()
            return
        }
        if attachedToExistingProcess,
           HvfAttachedLivenessSchedule.isDue(now: Date(), next: nextAttachedLivenessCheck) {
            nextAttachedLivenessCheck = HvfAttachedLivenessSchedule.next(after: Date())
            if !processIsRunning(config.targetDiskPath) {
                markStopped()
                return
            }
        }
        let connected: Bool
        if case .connected = connectionState { connected = true } else { connected = false }
        inputDriver.poll(binding: inputBinding, serviceReady: serviceStarted && connected, lines: lines) { [self] command in
            guard command.utf8.count <= 87_434, !command.contains("\n"),
                  !command.contains("\r"), !command.contains("\0") else { return false }
            return appendControlCommand(command)
        }
        if case .stopping = connectionState {
            sendGracefulStopIfReady()
            if let stopDeadline, Date() >= stopDeadline {
                append(.unknown("graceful shutdown timed out; terminating the wrapper"))
                if let process {
                    process.terminate()
                } else if attachedToExistingProcess {
                    Shell.killProcesses(matching: config.targetDiskPath)
                }
                self.stopDeadline = nil
            }
        }
    }

    private func handle(_ event: BvAgentEvent) {
        append(event)
        switch event {
        case let .ready(host, _):
            if connectionState != .stopping {
                connectionState = .connected(host: host)
            }
        case .serviceStart:
            serviceStarted = true
            sendGracefulStopIfReady()
        case .aliveHeartbeat:
            lastHeartbeatDate = Date()
            lastHeartbeatAge = 0
        default:
            break
        }
    }

    private func sendGracefulStopIfReady() {
        guard case .stopping = connectionState, serviceStarted, !stopCommandSent else { return }
        stopCommandSent = true
        if sendCtl("shutdown.exe /p /f") {
            append(.unknown("graceful guest shutdown requested"))
            return
        }
        append(.unknown("graceful guest shutdown unavailable; terminating the HVF wrapper"))
        if let process {
            process.terminate()
        } else if attachedToExistingProcess {
            Shell.killProcesses(matching: config.targetDiskPath)
        }
        stopDeadline = nil
    }

    private func markStopped() {
        if let process, let identity = ownedProcessIdentity, !process.isRunning {
            lastOwnedExit = HvfOwnedRuntimeExit(identity: identity, process: process)
        }
        ownedProcessIdentity = nil
        pendingPaste = nil
        inputDriver.attachUnknown(binding: inputBinding)
        timer?.invalidate()
        timer = nil
        process = nil
        closeLiveInput()
        attachedToExistingProcess = false
        connectionState = .stopped
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        stopCommandSent = false
        stopDeadline = nil
    }

    private func resetObservedRuntimeState(clearEvents: Bool) {
        pendingPaste = nil
        inputDriver.attachUnknown(binding: inputBinding)
        tailReader = TailOffsetReader()
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        stopCommandSent = false
        stopDeadline = nil
        liveInputWriteFailureReported = false
        if clearEvents { events = [] }
    }

    private func append(_ event: BvAgentEvent) {
        events.append(event)
        if events.count > 500 {
            events.removeFirst(events.count - 500)
        }
    }

    private var inputBinding: [String] {
        [config.targetDiskPath, config.uefiVarsPath, config.evidenceDir, config.ctlFilePath]
    }

    func beginOwnedInputBoot() {
        pendingPaste = nil
        inputDriver.beginOwnedBoot(binding: inputBinding)
    }

    func cancelOrderedInputTarget() { inputDriver.cancelTarget() }

}

#if canImport(AppKit)
#endif
