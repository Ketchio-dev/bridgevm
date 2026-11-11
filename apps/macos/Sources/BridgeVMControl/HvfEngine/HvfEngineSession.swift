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
    let heartbeat = HvfRuntimeHeartbeat() // publishes the per-poll heartbeat age apart from the session
    let shareRootNotice = HvfShareRootNotice()
    let agentUpdateNotice = HvfAgentUpdateNotice()
    let fileDrops = HvfFileDropQueue() // Finder drops copied to the guest Desktop
    let clipboardImageNotice = HvfClipboardImageNotice()
    let agentSilenceNotice = HvfAgentSilenceNotice() // the guest agent is not there (HvfEngineSessionAgentSilence)
    @Published var events: [BvAgentEvent] = []
    @Published var displayDriverFlagMessage: String? // why HvfDisplayDriverFlag could not save yet
    var repoRoot: URL
    var workAdmission: LibraryWorkAdmission?
    var reservedWorkAdmission: (@MainActor (UUID, Bool) -> String?)?
    var onConfigurationPersisted: (@MainActor (_ previous: VMConfig, _ saved: VMConfig) -> Void)? // library rebind + reload
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
    @Published var forceStopTarget: HvfForceStopTarget?
    @Published var stopWait: HvfGuestStopWait? // what Windows is doing while a Stop waits for it
    @Published var restartRequest: HvfPendingRestart? // a Restart or Reset sent this boot (HvfGuestRestart)
    @Published var resetConfirmation: HvfResetConfirmation? // the Reset the user is asked to confirm
    @Published var restartNotice: String? // a Reset that no helper took
    @Published var guestBoot = 0 // bumps at every guest reset record and at every run start or end
    @Published var pause = HvfPauseState.running { // what the probe last said about Pause (HvfGuestPause)
        didSet { guestRunClock.hold(pause.holdsInput, host: hostNow()) }
    }
    var hostNow: () -> Date = Date.init // the host clock behind guestNow()
    var guestRunClock = HvfGuestRunClock()
    @Published var pauseNotice: String? // a Pause or Resume that did not happen, or a full Mac disk's pause
    var diskFullReported = false // `BVSTORAGE full`: the `BVPAUSE paused` that follows is for a full Mac disk
    var runtimeTransitionInProgress = false
    var timer: Timer?
    var tailReader = TailOffsetReader()
    var agentEvents = BvAgentEvent.Stream() // run.log's lines across polls, with tailReader
    var lastHeartbeatDate: Date?
    var serviceStarted = false
    var hardwareCursorOffSent = false // HWCursor=0 went to the guest after the setting was turned off
    // Windows links (HvfGuestLinks.swift): the registration's removal went to the guest after the setting was
    // turned off, the last link opened, and what opens it.
    var guestLinksOffSent = false
    var lastGuestLinkOpen: Date?
    /// Links the probe logged before this session existed are never opened: a reattached session reads run.log
    /// from its start, and a link younger than the freshness window would open a second time.
    let guestLinksNotBefore = Date()
    var guestLinkOpener: any HvfURLOpener = HvfWorkspaceURLOpener()
    let guestLinkNotice = HvfGuestLinkNotice()
    @Published var diskGrowNotice: String? // the guest's report on extending C: after a disk grow (HvfDiskGrow)
    var diskExtendInFlight = false
    var legacyPaste = HvfLegacyPasteState() // a pending clipboard paste and display input held behind it
    let inputDriver = HvfSessionInputDriver()
    private var scrollAccumulator = HvfScrollAccumulator() // the scroll fraction not yet sent to the guest
    let appPacing = HvfAppPacingRecords() // the session's app-side display and input records
    var textInputTarget: HvfTextInputTargetIdentity { .init(owner: ObjectIdentifier(self), generation: inputDriver.textInputGeneration, binding: inputBinding) }
    var stopCommandSent = false
    var stopDeadline: Date?
    var attachedToExistingProcess = false
    var nextAttachedLivenessCheck = Date.distantPast
    private let liveInputFile = HvfLiveInputFile()
    var liveInputWriteFailureReported = false
    var displayResize = HvfDisplayResizeRequest() // the display window size the guest display should follow
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
        // The 10 ms input pump waits out a pause: no receipt comes before the guest runs again.
        inputDriver.onPoll = { [weak self] in if self?.pause == .running { self?.poll() } }
        inputDriver.now = { [weak self] in self?.guestNow() ?? Date() }
        inputDriver.onDiagnostic = { [weak self] in self?.append(.unknown($0)) }
        appPacing.observe(inputDriver)
    }

    deinit {
        timer?.invalidate()
        if ownedController == nil { process?.terminate() }
        liveInputFile.close()
    }

    func ownedRuntimeChanged(token: UUID) { // runs on every controller tick: assign only a changed state
        guard let controller = ownedController, controller.identity.token == token else { return }
        refreshOwnedStart()
        lastOwnedExit = controller.runnerExit
        if controller.teardownConfirmed { markStopped() }
        else if controller.failure != nil { if connectionState != .timedOut { connectionState = .timedOut } }
        else if controller.operation != nil { if connectionState != .stopping { connectionState = .stopping } }
        refreshForceStopTarget()
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
            _ = requestOwnedStop(target: controller.identity, operationID: UUID(), afterGrace: .offerForceStop)
            return existing ? .alreadyStopping(deadline: nil) : .requested(deadline: nil)
        }
        if connectionState == .stopping { return .alreadyStopping(deadline: stopDeadline) }
        let ownsRunningProcess = process?.isRunning == true
        let attachedProcessIsRunning = attachedToExistingProcess && processIsRunning(config.targetDiskPath)
        guard ownsRunningProcess || attachedProcessIsRunning else {
            markStopped()
            return .alreadyStopped
        }
        resumeBeforeStop()
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
            try HvfControlFileAppend.append(data, to: URL(fileURLWithPath: path))
        } catch {
            append(.unknown("control command write failed: \(error.localizedDescription)"))
            return false
        }
        return true
    }

    @discardableResult func sendKey(_ action: String) -> Bool {
        guard !pause.holdsInput else { return false }
        guard legacyPaste.pending == nil else { append(.unknown("key input refused: clipboard paste pending")); return false }
        let admission = inputDriver.route(.key(action), binding: inputBinding)
        guard admission == .legacy else { return admission == .queued }
        return appendLiveInput("KEY \(action)")
    }

    @discardableResult
    func sendText(_ value: String) -> HvfTextInputSubmission {
        guard !guiStartCleanupOnly, !pause.holdsInput else { return .refused }
        guard legacyPaste.pending == nil else { append(.unknown("text input refused: clipboard paste pending")); return .refused }
        guard !value.isEmpty else { return .refused }
        switch inputDriver.route(.text(value), binding: inputBinding) {
        case .queued: return .acceptedForProcessing
        case .refused: return .refused
        case .legacy: break
        }
        let plan = HvfTextInputPlan.make(for: value)
        for chunk in plan.hidChunks {
            guard appendLiveInput("KEY text-hex:\(chunk)") else { return .refused }
        }
        if let base64 = plan.clipboardBase64 {
            guard serviceStarted, case .connected = connectionState else {
                append(.unknown("clipboard paste refused: guest service is not connected")); return .refused
            }
            let request = HvfClipboardPaste(base64: base64, now: Date())
            guard request.fitsGuestCommandLine(reportingTo: self), sendCtl(request.command) else { return .refused }
            legacyPaste.pending = request
            return .acceptedForProcessing
        }
        return .legacyAttempted
    }

    #if canImport(AppKit)
    func sendPointerClick(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction(.click, location: location, viewSize: viewSize, imageSize: imageSize)
    }

    /// `modifiers` are the guest modifiers held for the action (HvfHostKeyMapping.pointerModifiers); they reach
    /// only an agent that takes them, and are dropped otherwise, as before.
    @discardableResult func sendPointerPress(location: CGPoint, viewSize: CGSize, imageSize: CGSize, modifiers: String = "") -> String {
        sendPointerAction(.press, location: location, viewSize: viewSize, imageSize: imageSize, modifiers: modifiers)
    }

    func sendPointerMove(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction(.move, location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerRelease(location: CGPoint, viewSize: CGSize, imageSize: CGSize, modifiers: String = "") {
        sendPointerAction(.release, location: location, viewSize: viewSize, imageSize: imageSize, modifiers: modifiers)
    }

    func sendPointerRightClick(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction(.rightClick, location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerRightPress(location: CGPoint, viewSize: CGSize, imageSize: CGSize, modifiers: String = "") {
        sendPointerAction(.rightPress, location: location, viewSize: viewSize, imageSize: imageSize, modifiers: modifiers)
    }

    /// Only for an agent that takes the middle button (`guestMiddleButton`); the release is the common one.
    func sendPointerMiddlePress(location: CGPoint, viewSize: CGSize, imageSize: CGSize, modifiers: String = "") {
        guard guestMiddleButton else { return }
        sendPointerAction(.middlePress, location: location, viewSize: viewSize, imageSize: imageSize, modifiers: modifiers)
    }

    /// An agent that advertised SCROLL_UNITS gets raw wheel units on both axes; an older agent and the
    /// VMM's HID wheel get whole vertical notches. True when a scroll request was formed.
    @discardableResult
    func sendPointerScroll(_ delta: HvfScrollAccumulator.Delta, location: CGPoint, viewSize: CGSize, imageSize: CGSize,
                           modifiers: String = "") -> Bool {
        guard !pause.holdsInput, let fit = HvfDisplayFit(viewSize: viewSize, imageSize: imageSize),
              let point = mappedPointer(location, viewSize: viewSize, imageSize: imageSize) else { return false }
        let units = inputDriver.router.scrollUnits
        guard let scroll = scrollAccumulator.add(delta, guestPixelsPerPoint: 1 / fit.scale, wholeNotches: !units) else { return false }
        let at = "@\(point.x)x\(point.y)", held = heldModifiers(modifiers)
        if units {
            _ = inputDriver.route(.pointer("scrollunits:\(scroll.vertical),\(scroll.horizontal)" + at + held), binding: inputBinding)
            return true
        }
        let command = "scroll:\(scroll.vertical / HvfScrollAccumulator.notch)" + at
        if inputDriver.route(.pointer(command + held), binding: inputBinding) == .legacy { appendLiveInput("POINTER " + command) }
        return true
    }

    /// Returns where the event went (queued, legacy, refused or unmapped) for the display's diagnostic.
    @discardableResult private func sendPointerAction(_ action: HvfPointerAction, location: CGPoint, viewSize: CGSize, imageSize: CGSize,
                                                      modifiers: String = "") -> String {
        guard !pause.holdsInput else { return "paused" }
        guard let point = mappedPointer(location, viewSize: viewSize, imageSize: imageSize) else { return "unmapped" }
        if action.mayMoveGuestFocus { cancelPendingPaste("pointer pressed") }
        let admission = inputDriver.route(.pointer("\(action.orderedVerb):\(point.x)x\(point.y)" + heldModifiers(modifiers)),
                                          binding: inputBinding)
        if admission != .legacy { return admission == .queued ? "queued" : "refused-" + inputDriver.routeDiagnostic(attached: attachedToExistingProcess) }
        appendLiveInput("POINTER \(action.rawValue):\(point.x)x\(point.y)"); return "legacy-" + inputDriver.routeDiagnostic(attached: attachedToExistingProcess)
    }

    /// The `:<modifiers>` field for an agent that takes held modifiers on a pointer action, else nothing. Each
    /// request presses and releases them itself, so a drag's press and release carry their own.
    private func heldModifiers(_ modifiers: String) -> String {
        modifiers.isEmpty || !inputDriver.router.pointerModifiers ? "" : ":" + modifiers
    }

    private func mappedPointer(_ location: CGPoint, viewSize: CGSize, imageSize: CGSize) -> (x: UInt16, y: UInt16)? {
        HvfDisplayCoordinates.absolutePointer(location: location, viewSize: viewSize, imageSize: imageSize)
    }
    #endif

    @discardableResult func appendLiveInput(_ line: String) -> Bool {
        guard !guiStartCleanupOnly, inputDriver.allowLegacyWrite(binding: inputBinding) else { return false }
        return writeLiveInput(line)
    }

    /// The display view's size in points and its window's backing scale factor; `settled` (live resize
    /// ended) sends it without the debounce.
    func displayViewResized(to points: CGSize, scale: CGFloat = 1, settled: Bool) {
        displayResize.retina = config.retinaResizeSetting
        displayResize.viewResized(to: .init(points: points, scale: scale), at: ProcessInfo.processInfo.systemUptime)
        if settled {
            displayResize.settle()
            flushDisplayResize()
        }
    }

    private func flushDisplayResize() {
        displayResize.retina = config.retinaResizeSetting
        guard let size = displayResize.take(at: ProcessInfo.processInfo.systemUptime),
              appendDisplayResize(size) else { return }
        displayResize.didSend(size)
    }

    private func appendDisplayResize(_ size: HvfDisplayResizeRequest.Size) -> Bool {
        appendVMMControl(size.command)
    }

    /// True while a running VM's VMM takes requests on this run's input channel.
    var vmmControlAvailable: Bool {
        switch connectionState {
        case .booting, .connected: break
        case .stopped, .stopping, .timedOut: return false
        }
        return !guiStartCleanupOnly && inputDriver.router.binding == inputBinding
    }

    /// `RESIZE` and `RESET` are requests to the VMM, not guest input: they bypass the ordered input
    /// stream (`allowLegacyWrite` would refuse them or reset that stream) and check only that this run's
    /// input channel is still the bound one.
    func appendVMMControl(_ line: String) -> Bool {
        vmmControlAvailable && writeLiveInput(line)
    }

    /// `RESUME`, which a Stop or Force Stop already under way still sends: only this run's input
    /// channel takes it.
    func appendRunControl(_ line: String) -> Bool {
        !guiStartCleanupOnly && inputDriver.router.binding == inputBinding && writeLiveInput(line)
    }

    private func writeLiveInput(_ line: String) -> Bool {
        let path = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("input.ctl")
        guard let data = "\(line)\n".data(using: .utf8) else { return false }
        do {
            try liveInputFile.append(data, to: path)
            liveInputWriteFailureReported = false
            return true
        } catch {
            if !liveInputWriteFailureReported {
                append(.unknown("live input write failed: \(error.localizedDescription)"))
                liveInputWriteFailureReported = true
            }
            return false
        }
    }

    /// One press of the guest's ACPI power button, read by the VMM from the live input channel.
    func pressGuestPowerButton() -> Bool { writeLiveInput("POWERBUTTON") }

    func closeLiveInput() {
        liveInputFile.close()
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
        finishLegacyPaste(lines: lines)
        for event in agentEvents.parse(lines) {
            handle(event)
        }
        expireUnacknowledgedReset()
        expireUnansweredPause()
        refreshAgentSilence()
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
        flushDisplayResize()
        inputDriver.poll(binding: inputBinding, serviceReady: serviceStarted && guestTakesInput, lines: lines) { [self] command in
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

    func markStopped() {
        guard !runtimeTransitionInProgress, !mayHaveOwnedWork else { return }
        runtimeTransitionInProgress = true
        appPacing.end(evidenceDir: config.evidenceDir)
        if let process, let identity = ownedProcessIdentity, !process.isRunning {
            lastOwnedExit = HvfOwnedRuntimeExit(identity: identity, process: process)
        }
        ownedProcessIdentity = nil
        legacyPaste = HvfLegacyPasteState()
        inputDriver.attachUnknown(binding: inputBinding)
        timer?.invalidate()
        timer = nil
        process = nil
        closeLiveInput()
        attachedToExistingProcess = false
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        shareRootNotice.reset()
        agentUpdateNotice.reset()
        fileDrops.reset()
        clipboardImageNotice.reset()
        stopCommandSent = false
        stopDeadline = nil
        if forceStopTarget != nil { forceStopTarget = nil }
        if stopWait != nil { stopWait = nil }
        guestBootChanged()
        guiStartCleanupOnly = false
        connectionState = .stopped
        runtimeTransitionInProgress = false
        // vm.json may change again: a display driver installed during this run is saved now,
        // whether or not the VM's view is open (a guest shutdown, another VM selected).
        HvfDisplayDriverFlag.saveIfStopped(self)
    }

    func resetObservedRuntimeState(clearEvents: Bool) {
        if !hasPendingGUIStart { guiStartOperation = nil }
        appPacing.begin()
        legacyPaste = HvfLegacyPasteState()
        inputDriver.attachUnknown(binding: inputBinding)
        tailReader = TailOffsetReader()
        agentEvents = BvAgentEvent.Stream()
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        shareRootNotice.reset()
        agentUpdateNotice.reset()
        fileDrops.reset()
        clipboardImageNotice.reset()
        stopCommandSent = false
        stopDeadline = nil
        guestBootChanged()
        liveInputWriteFailureReported = false
        displayResize.deviceRestarted() // a new run's display starts at its default size; an attached one is unknown
        if clearEvents { events = [] }
    }

    /// Windows keeps its input while a Stop waits for it to shut down: the user may still have to save
    /// or answer Windows' "apps are preventing shutdown" screen. Force Stop or cancellation ends that.
    /// A Stop keeps an open ordered channel but never opens one: a new negotiation would write to a
    /// control file the Stop may have found replaced. Until then input uses the legacy device path.
    private var guestTakesInput: Bool {
        if case .connected = connectionState { return true }
        return connectionState == .stopping && inputDriver.router.activated
            && ownedController?.waitsForGuestShutdown == true
    }

    private var inputBinding: [String] {
        [config.targetDiskPath, config.uefiVarsPath, config.evidenceDir, config.ctlFilePath]
    }

    func beginOwnedInputBoot() {
        legacyPaste = HvfLegacyPasteState()
        inputDriver.beginOwnedBoot(binding: inputBinding)
    }

    func cancelOrderedInputTarget() {
        cancelPendingPaste("input target changed")
        inputDriver.cancelTarget()
    }

    /// Drops a legacy paste before its delayed `KEY ctrl+v` can reach a different guest target.
    private func cancelPendingPaste(_ reason: String) {
        if let message = legacyPaste.cancel(reason) { append(.unknown(message)) }
    }
}
