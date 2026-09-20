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
    @Published var config: HvfEngineConfig { didSet { runtimeConfigurationChanged() } }
    @Published var connectionState: HvfConnectionState = .stopped
    @Published var lastHeartbeatAge: TimeInterval?
    @Published var events: [BvAgentEvent] = []
    var repoRoot: URL
    var workAdmission: LibraryWorkAdmission?
    var reservedWorkAdmission: (@MainActor (UUID, Bool) -> String?)?
    var ownedStartOperation: HvfOwnedStartOperation?
    var ownedStartExecution: HvfOwnedStartExecution?
    var guiStartOperation: HvfGUIStartOperation?
    var guiStartExecution: HvfGUIStartExecution?
    var guiStartCleanupOnly = false
    var guiStartWorker: HvfGUIStartExecution.Worker = HvfGUIStartWorker.run
    var mutationReservation: HvfRuntimeMutationReservation?
    var ownedStartNow: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }
    var ownedStartWorker: HvfOwnedStartExecution.Worker = HvfOwnedStartExecution.run
    let workAdmissionGate = HvfRuntimeWorkAdmissionGate()
    var process: Process?
    var ownedController: HvfOwnedRunController?
    var runtimeTransitionInProgress = false
    var timer: Timer?
    var tailReader = TailOffsetReader()
    var lastHeartbeatDate: Date?
    var serviceStarted = false
    private var pendingPaste: HvfClipboardPaste?
    private let inputDriver = HvfSessionInputDriver()
    var stopCommandSent = false
    var stopDeadline: Date?
    var attachedToExistingProcess = false
    var nextAttachedLivenessCheck = Date.distantPast
    private var liveInputHandle: FileHandle?
    private var liveInputPath: URL?
    var liveInputWriteFailureReported = false
    var processLaunch: (Process) throws -> Void = { try $0.run() }
    let processIsRunning: (String) -> Bool
    var ownedProcessIdentity: HvfOwnedRuntimeIdentity?
    var lastOwnedExit: HvfOwnedRuntimeExit?
    let vtpmKeyProvider: VTPMStateKeyProviding

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
        if ownedController == nil { process?.terminate() }
        try? liveInputHandle?.close()
    }

    func requestOwnedStop(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopAdmission {
        guard let ownedController else { return .refused(.supervisionUnavailable) }
        let result = ownedController.requestStop(target: target, operationID: operationID)
        if case .refused = result { return result }
        cancelOrderedInputTarget()
        ownedRuntimeChanged(token: ownedController.identity.token)
        return result
    }

    func ownedStopObservation(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopObservation? {
        ownedController?.observation(target: target, operationID: operationID)
    }

    func ownedRuntimeChanged(token: UUID) {
        guard let controller = ownedController, controller.identity.token == token else { return }
        refreshOwnedStart()
        lastOwnedExit = controller.runnerExit
        if controller.teardownConfirmed { markStopped() }
        else if controller.failure != nil { connectionState = .timedOut }
        else if controller.operation != nil { connectionState = .stopping }
    }

    @discardableResult
    func stopOwned(expectedToken: UUID) -> HvfRuntimeStopOutcome {
        if ownedProcessIdentity == nil, lastOwnedExit?.identity.token == expectedToken { return .alreadyStopped }
        guard ownedProcessIdentity?.token == expectedToken, process != nil else { return .notOwned }
        return requestStop()
    }

    private func requestStop() -> HvfRuntimeStopOutcome {
        guard !runtimeStartupWorkerPending else { return .notOwned }
        if let controller = ownedController {
            let existing = controller.operation != nil
            _ = requestOwnedStop(target: controller.identity, operationID: UUID())
            return existing ? .alreadyStopping(deadline: nil) : .requested(deadline: nil)
        }
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
    func attachToRunningVM(reportRefusal: Bool = true, reportDuplicateLaunch: Bool = false) -> Bool {
        guard !runtimeTransitionInProgress, !hasPendingRuntimeStart, !hasPendingRuntimeMutation, !mayHaveOwnedWork else { return false }
        guard workAdmissionGate.check(workAdmission, reportRefusal: reportRefusal) == nil else { return false }
        guard process?.isRunning != true else { return false }
        guard processIsRunning(config.targetDiskPath) else { return false }
        if process != nil { markStopped() }
        runtimeTransitionInProgress = true
        defer { runtimeTransitionInProgress = false }
        adoptObservedAttachment(reportDuplicateLaunch: reportDuplicateLaunch)
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
        guard !guiStartCleanupOnly else { return false }
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
        guard !guiStartCleanupOnly else { return .refused }
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
        guard !guiStartCleanupOnly else { return }
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

    func closeLiveInput() {
        try? liveInputHandle?.close()
        liveInputHandle = nil
        liveInputPath = nil
    }

    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
        poll()
    }

    func poll() {
        if guiStartCleanupOnly { pollGUIStartCleanup(); return }
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
        if let controller = ownedController {
            controller.tick()
            if controller.teardownConfirmed { return }
        } else if let process, !process.isRunning {
            markStopped(); return
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
        if ownedController == nil, case .stopping = connectionState {
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
            if connectionState != .stopping, ownedController?.failure == nil {
                connectionState = .connected(host: host)
            }
        case .serviceStart:
            serviceStarted = true
            if let ownedController { ownedController.guestServiceReady() }
            else { sendGracefulStopIfReady() }
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

    func markStopped() {
        guard !runtimeTransitionInProgress, !mayHaveOwnedWork else { return }
        runtimeTransitionInProgress = true
        defer { runtimeTransitionInProgress = false }
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
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        stopCommandSent = false
        stopDeadline = nil
        guiStartCleanupOnly = false
        connectionState = .stopped
    }

    func resetObservedRuntimeState(clearEvents: Bool) {
        if !hasPendingGUIStart { guiStartOperation = nil }
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

    func append(_ event: BvAgentEvent) {
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
