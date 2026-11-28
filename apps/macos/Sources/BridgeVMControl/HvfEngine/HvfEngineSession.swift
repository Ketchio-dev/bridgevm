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
    @Published var connectionState: HvfConnectionState = .stopped {
        didSet {
            updateSleepPrevention()
            // The snapshot card's last result is about the VM as it was before this start.
            if oldValue == .stopped, connectionState != .stopped { snapshotModel.clearResult() }
        }
    }
    let heartbeat = HvfRuntimeHeartbeat() // publishes the per-poll heartbeat age apart from the session
    let shareRootNotice = HvfShareRootNotice()
    let diskSyncNotice = HvfDiskSyncNotice()
    let snapshotModel = HvfWindowsSnapshotModel()
    let agentUpdateNotice = HvfAgentUpdateNotice()
    let fileDrops = HvfFileDropQueue() // Finder drops copied to the guest Desktop
    let clipboardImageNotice = HvfClipboardImageNotice()
    let portForwardNotice = HvfPortForwardNotice()
    let audioNotice = HvfAudioNotice()
    let agentSilenceNotice = HvfAgentSilenceNotice() // the guest agent is not there (HvfEngineSessionAgentSilence)
    @Published var events: [BvAgentEvent] = []
    @Published var driveDisconnected = false // the drive with this run's disk was unmounted (HvfDriveDisconnect)
    @Published var guestToolsMessage: String?
    @Published var guestToolsPreparing = false
    @Published var displayDriverFlagMessage: String? // why HvfDisplayDriverFlag could not save yet
    @Published var hostSettingsSaveMessage: String? // why a setting changed while Windows ran is not saved yet
    @Published var displayDriverNoticeDismissed = false // per VM, only for this app session
    @Published var displayDriverRestartRequired = false
    @Published var displayDriverUnbound = false
    @Published var displayDriverUnboundDismissed = false
    @Published var displayDriverRestarting = false
    /// Why Restart Now didn't restart: refused before the shutdown, or the start after it refused.
    @Published var displayDriverRestartNote: String?
    var repoRoot: URL
    weak var library: LibraryModel?
    var workAdmission: LibraryWorkAdmission?
    var reservedWorkAdmission: (@MainActor (UUID, Bool) -> String?)?
    var onConfigurationPersisted: (@MainActor (_ previous: VMConfig, _ saved: VMConfig) -> Void)? // library rebind + reload
    var guiStartOperation: HvfGUIStartOperation?
    var guiStartExecution: HvfGUIStartExecution?
    var guiStartCleanupOnly = false
    var guiStartWorker: HvfGUIStartExecution.Worker = HvfGUIStartWorker.run
    var mutationReservation: HvfRuntimeMutationReservation?
    var snapshotDeletions = 0 // HvfWindowsSnapshotExecution.deleteSnapshot, which needs no reservation
    var ownedStartNow: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }
    let workAdmissionGate = HvfRuntimeWorkAdmissionGate()
    /// Keyboard and mouse refuse input until an agent restart or guest reset (HvfSessionInputDriver);
    /// the display says so (HvfInputStoppedStrip) instead of only beeping.
    @Published var inputStopped = false
    let sleepPreventionID = UUID()
    var sleepPrevention = ControlAppPower.shared
    func updateSleepPrevention() {
        sleepPrevention.update(sleepPreventionID, active: needsMacAwake, ramMiB: config.ramMiB)
    }
    /// Cleanup can remain unconfirmed after a dead runner, but no Windows task is using the Mac.
    var needsMacAwake: Bool {
        guard pause != .paused else { return false }
        if runtimeStartupWorkerPending { return true }
        if let process { return process.isRunning }
        return hasRetainedAttachment && connectionState != .stopped
    }
    var process: Process?
    var ownedController: HvfOwnedRunController?
    @Published var forceStopTarget: HvfForceStopTarget?
    @Published var stopWait: HvfGuestStopWait? // what Windows is doing while a Stop waits for it
    @Published var restartRequest: HvfPendingRestart? // a Restart or Reset sent this boot (HvfGuestRestart)
    @Published var resetConfirmation: HvfResetConfirmation? // the Reset the user is asked to confirm
    @Published var restartNotice: String? // a Reset that no helper took
    @Published var guestBoot = 0 // bumps at every guest reset record and at every run start or end
    @Published var pause = HvfPauseState.running { // what the probe last said about Pause (HvfGuestPause)
        didSet {
            guestRunClock.hold(pause.holdsInput, host: hostNow())
            updateSleepPrevention()
        }
    }
    /// The host clock behind guestNow(): uptime (ownedStartNow), which stops while the Mac sleeps, as the guest does. On
    /// the wall clock an input receipt still awaited at sleep expired on wake and stopped all input.
    lazy var hostNow: () -> Date = { [unowned self] in Date(timeIntervalSinceReferenceDate: self.ownedStartNow()) }
    var guestRunClock = HvfGuestRunClock()
    @Published var pauseNotice: String? // a Pause or Resume that did not happen, or a full Mac disk's pause
    var diskFullReported = false // `BVSTORAGE full`: the `BVPAUSE paused` that follows is for a full Mac disk
    var runtimeTransitionInProgress = false
    var timer: Timer?
    var tailReader = TailOffsetReader()
    var agentEvents = BvAgentEvent.Stream() // run.log's lines across polls, with tailReader
    var lastHeartbeatDate: Date?
    var openSettingsSheets = Set<ObjectIdentifier>()
    var serviceStarted = false
    var guestStartApps = false // the agent's last READY carried `startapps` (HvfGuestStartApps)
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
    // Windows printing (HvfGuestPrint.swift): the printer's removal went to the guest after the setting was
    // turned off, the printer-failed notice was shown, the jobs waiting for a print panel, the one shown, and
    // what shows it.
    var printOffSent = false
    var printSetupFailureShown = false
    var printQueue: [HvfGuestPrint.Job] = []
    var printShowing: URL?
    var printPanelPresented = false
    var printGeneration = UUID()
    var printVerificationTask: Task<Void, Never>?
    var printVerifier: @Sendable (HvfGuestPrint.Job) -> Bool = HvfGuestPrint.verified
    var printPresenter: any HvfPrintPresenter = HvfPDFPrintPresenter()
    let printNotice = HvfGuestPrintNotice()
    @Published var diskGrowNotice: String? // the guest's report on extending C: after a disk grow (HvfDiskGrow)
    var diskExtendInFlight = false
    var legacyPaste = HvfLegacyPasteState() // a pending clipboard paste and display input held behind it
    weak var submittedKeyboardDraft: HvfTextDraftDelivery?
    var heldKeys = HvfHeldKeys() // Mac keys held down on the guest (HvfEngineSessionHeldKeys.swift)
    var guestCaretQuery: HvfGuestCaretQuery? // where the guest's text caret is, asked once per composition
    let inputDriver = HvfSessionInputDriver()
    private var scrollAccumulator = HvfScrollAccumulator() // the scroll fraction not yet sent to the guest
    let appPacing = HvfAppPacingRecords() // the session's app-side display and input records
    var textInputTarget: HvfTextInputTargetIdentity { .init(owner: ObjectIdentifier(self), generation: inputDriver.textInputGeneration, binding: inputBinding) }
    var stopCommandSent = false
    var stopDeadline: Date?
    var attachedToExistingProcess = false { didSet { attachedLivenessGeneration &+= 1 } }
    var nextAttachedLivenessCheck = Date.distantPast
    private var attachedLivenessGeneration: UInt64 = 0 // a late lookup answers for an earlier attachment
    private var attachedLivenessLookupInFlight = false
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
        inputDriver.now = { [weak self] in
            self?.guestNow() ?? Date(timeIntervalSinceReferenceDate: ProcessInfo.processInfo.systemUptime)
        }
        inputDriver.onDiagnostic = { [weak self] in self?.append(.unknown($0)) }
        inputDriver.onLegacyReplay = { [weak self] in self?.resendOnLegacy($0) }
        inputDriver.onLegacyBarrier = { [weak self] in _ = self?.appendLiveInput($0) }
        inputDriver.onInputStoppedChange = { [weak self] in self?.inputStopped = $0 }
        appPacing.observe(inputDriver)
    }

    deinit {
        timer?.invalidate()
        if ownedController == nil { process?.terminate() }
        liveInputFile.close()
        let id = sleepPreventionID, power = sleepPrevention
        Task { @MainActor in power.update(id, active: false) }
    }

    func ownedRuntimeChanged(token: UUID) { // runs on every controller tick: assign only a changed state
        defer { updateSleepPrevention() }
        guard let controller = ownedController, controller.identity.token == token else { return }
        lastOwnedExit = controller.runnerExit
        if controller.teardownConfirmed { markStopped() }
        else if controller.failure != nil, !(controller.supervisionLost && controller.operation == nil && controller.process.isRunning) {
            if connectionState != .timedOut { connectionState = .timedOut }
        }
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
        // Uptime, like the owned path: a Mac asleep during the shutdown must not use up the 180 s.
        stopDeadline = hostNow().addingTimeInterval(180)
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
        // Held keys go up first, each on its own route, so none stays down behind Windows' secure desktop.
        if HvfGuestInputEncoding.isSecureAttention(action) { releaseHeldKeys() }
        let admission = inputDriver.route(.key(action), binding: inputBinding)
        guard admission == .legacy else { return admission == .queued }
        return appendLiveInput("KEY \(action)")
    }

    @discardableResult
    func sendText(_ value: String, draft: HvfTextDraftDelivery? = nil) -> HvfTextInputSubmission {
        guard !guiStartCleanupOnly, !pause.holdsInput else { return .refused }
        guard legacyPaste.pending == nil else { append(.unknown("text input refused: clipboard paste pending")); return .refused }
        guard !value.isEmpty else { return .refused }
        switch inputDriver.route(.text(value, draft: draft), binding: inputBinding) {
        case .queued: return .acceptedForProcessing
        case .refused: return .refused
        case .legacy: return sendLegacyText(value, draft: draft)
        }
    }

    /// Text on input.ctl: HID keys for printable ASCII, a clipboard paste for anything else.
    @discardableResult func sendLegacyText(_ value: String, draft: HvfTextDraftDelivery? = nil) -> HvfTextInputSubmission {
        let plan = HvfTextInputPlan.make(for: value)
        for chunk in plan.hidChunks {
            guard appendLiveInput("KEY text-hex:\(chunk)") else { return .refused }
        }
        if let base64 = plan.clipboardBase64 {
            guard serviceStarted, case .connected = connectionState else {
                append(.unknown("clipboard paste refused: guest service is not connected")); return .refused
            }
            var request = clipboardPaste(base64: base64, preferRelay: draft?.multiline == true)
            guard request.fitsGuestCommandLine(reportingTo: self), sendCtl(request.command) else { return .refused }
            if let draft { request.onPasteSent = { draft.onSent?() } }
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
    /// only an agent that takes them, and are dropped otherwise.
    @discardableResult func sendPointerPress(location: CGPoint, viewSize: CGSize, imageSize: CGSize, modifiers: String = "") -> String {
        sendPointerAction(.press, location: location, viewSize: viewSize, imageSize: imageSize, modifiers: modifiers)
    }

    func sendPointerMove(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction(.move, location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerRelease(location: CGPoint, viewSize: CGSize, imageSize: CGSize, modifiers: String = "", button: Int = 0) {
        let action: HvfPointerAction = button == 1 ? .rightRelease : button == 2 ? .middleRelease : .release
        sendPointerAction(action, location: location, viewSize: viewSize, imageSize: imageSize, modifiers: modifiers)
    }

    func sendPointerRightClick(location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        sendPointerAction(.rightClick, location: location, viewSize: viewSize, imageSize: imageSize)
    }

    func sendPointerRightPress(location: CGPoint, viewSize: CGSize, imageSize: CGSize, modifiers: String = "") {
        sendPointerAction(.rightPress, location: location, viewSize: viewSize, imageSize: imageSize, modifiers: modifiers)
    }

    /// Only for an agent that takes the middle button (`guestMiddleButton`).
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
        appendLiveInput("POINTER \(action.legacyVerb):\(point.x)x\(point.y)"); return "legacy-" + inputDriver.routeDiagnostic(attached: attachedToExistingProcess)
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
        guard !guiStartCleanupOnly, inputDriver.allowLegacyWrite(line, binding: inputBinding) else { return false }
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

    /// The Retina setting changed while the view stays: the size for the new setting goes out now.
    func displayResizeSettingChanged() {
        displayResize.settingChanged(retina: config.retinaResizeSetting)
        flushDisplayResize()
    }

    private func flushDisplayResize() {
        displayResize.retina = config.retinaResizeSetting
        guard let size = displayResize.take(at: ProcessInfo.processInfo.systemUptime, fallback: savedDisplaySize),
              appendDisplayResize(size) else { return }
        displayResize.didSend(size)
    }

    /// With no laid-out window, restore the saved pixel mode after each service start.
    /// Keep Windows' DPI until a window supplies its actual backing scale.
    private var savedDisplaySize: HvfDisplayResizeRequest.Size? {
        guard serviceStarted, !attachedToExistingProcess,
              let mode = HvfGuestResolutionPicker.firmwareMode(config.guestResolution) else { return nil }
        let parts = mode.split(separator: "x")
        guard let width = Int(parts[0]), let height = Int(parts[1]) else { return nil }
        return .init(width: width, height: height)
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
        defer { updateSleepPrevention() }
        if guiStartCleanupOnly { pollGUIStartCleanup(); return }
        let logURL = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("run.log")
        let lines = tailReader.readNewLines(from: logURL)
        if lines.contains(where: { $0.hasPrefix("BVGUESTTOOLS failed") }) { guestToolsMessage = HvfGuestTools.failed }
        else if lines.contains(where: { $0.hasPrefix("BVGUESTTOOLS attached") }) { guestToolsMessage = HvfGuestTools.attached }
        finishLegacyPaste(lines: lines)
        answerGuestCaret(lines: lines)
        for event in agentEvents.parse(lines) {
            handle(event)
        }
        expireUnacknowledgedReset()
        expireUnansweredPause()
        refreshAgentSilence()
        if let lastHeartbeatDate { // uptime, so time the Mac slept does not count
            lastHeartbeatAge = hostNow().timeIntervalSince(lastHeartbeatDate)
        }
        if let controller = ownedController {
            controller.tick()
            if controller.teardownConfirmed { return }
        } else if let process, !process.isRunning {
            markStopped(); return
        }
        if attachedToExistingProcess, !attachedLivenessLookupInFlight,
           HvfAttachedLivenessSchedule.isDue(now: Date(), next: nextAttachedLivenessCheck) {
            nextAttachedLivenessCheck = HvfAttachedLivenessSchedule.next(after: Date())
            lookUpAttachedLiveness()
        }
        flushDisplayResize()
        inputDriver.poll(binding: inputBinding, serviceReady: serviceStarted && guestTakesInput,
                         legacyPending: legacyPaste.pending != nil, lines: lines) { [self] command in
            guard command.utf8.count <= 87_434, !command.contains("\n"),
                  !command.contains("\r"), !command.contains("\0") else { return false }
            return appendControlCommand(command)
        }
        sendDueGuestCaret()
        if ownedController == nil, case .stopping = connectionState {
            sendGracefulStopIfReady()
            if let stopDeadline, hostNow() >= stopDeadline {
                append(.unknown("graceful shutdown timed out; terminating the wrapper"))
                if let process {
                    process.terminate()
                } else if attachedToExistingProcess {
                    // pgrep, waited for: off the main actor, like the liveness lookup below.
                    let disk = config.targetDiskPath
                    Task.detached { Shell.killProcesses(matching: disk) }
                }
                self.stopDeadline = nil
            }
        }
    }

    /// An attached VM has no Process handle: the lookup reads the disk's writer lock and waits for pgrep
    /// (tens of ms), which hitched the VM window every second on the main actor. One runs at a time.
    private func lookUpAttachedLiveness() {
        attachedLivenessLookupInFlight = true
        let generation = attachedLivenessGeneration, disk = config.targetDiskPath, isRunning = processIsRunning
        Task.detached { [weak self] in
            let running = isRunning(disk)
            await self?.finishAttachedLivenessLookup(running: running, generation: generation)
        }
    }

    private func finishAttachedLivenessLookup(running: Bool, generation: UInt64) {
        attachedLivenessLookupInFlight = false
        guard !running, generation == attachedLivenessGeneration else { return }
        markStopped()
        updateSleepPrevention()
    }

    func markStopped() {
        guard !runtimeTransitionInProgress, !mayHaveOwnedWork else { return }
        runtimeTransitionInProgress = true
        resetPendingPrintJobs()
        appPacing.end(evidenceDir: config.evidenceDir)
        if let process, let identity = ownedProcessIdentity, !process.isRunning {
            lastOwnedExit = HvfOwnedRuntimeExit(identity: identity, process: process)
        }
        ownedProcessIdentity = nil
        legacyPaste = HvfLegacyPasteState()
        heldKeys = HvfHeldKeys()
        inputDriver.attachUnknown(binding: inputBinding)
        // The terminal receipt can arrive before the next log poll. Drain the stopped helper's
        // final diagnostics before cancelling polling, including a final-save failure.
        // Owned cleanup has reaped the helper; legacy callers also observed exit. Cap at
        // 64 one-MiB reads anyway so an unexpected external appender cannot spin this actor.
        let logURL = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("run.log")
        for _ in 0..<64 {
            let lines = tailReader.readNewLines(from: logURL)
            for event in agentEvents.parse(lines) { diskSyncNotice.consume(event) }
            if tailReader.bytesReadLastCall == 0 { break }
        }
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
        HvfHostSettingLiveSave.saveIfStopped(self)
        displayDriverRestartRequired = false
    }

    func resetObservedRuntimeState(clearEvents: Bool) {
        resetPendingPrintJobs()
        if !hasPendingGUIStart { guiStartOperation = nil }
        appPacing.begin()
        legacyPaste = HvfLegacyPasteState()
        heldKeys = HvfHeldKeys()
        inputDriver.attachUnknown(binding: inputBinding)
        tailReader = TailOffsetReader()
        agentEvents = BvAgentEvent.Stream()
        displayDriverUnboundDismissed = false
        displayDriverUnbound = false
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        shareRootNotice.reset(missingFolder: config.missingShareFolder())
        portForwardNotice.reset()
        audioNotice.reset()
        diskSyncNotice.reset()
        agentUpdateNotice.reset()
        fileDrops.reset()
        clipboardImageNotice.reset()
        // A new run sets the printer and links up again and reports afresh.
        printNotice.show(nil)
        printSetupFailureShown = false
        guestLinkNotice.show(nil)
        stopCommandSent = false
        stopDeadline = nil
        guestBootChanged()
        liveInputWriteFailureReported = false
        driveDisconnected = false
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

    var inputBinding: [String] {
        [config.targetDiskPath, config.uefiVarsPath, config.evidenceDir, config.ctlFilePath]
    }

    func beginOwnedInputBoot() {
        legacyPaste = HvfLegacyPasteState()
        heldKeys = HvfHeldKeys()
        inputDriver.beginOwnedBoot(binding: inputBinding)
    }

    /// Focus loss, Pause, Stop and the display's other target changes; held keys and buttons go up.
    func cancelOrderedInputTarget() {
        cancelPendingPaste("input target changed")
        releaseHeldKeysForTargetChange()
        inputDriver.cancelTarget()
    }

    /// Drops a legacy paste before its delayed `KEY ctrl+v` can reach a different guest target.
    func cancelPendingPaste(_ reason: String) {
        if let message = legacyPaste.cancel(reason) { append(.unknown(message)) }
    }
}

@MainActor
extension HvfEngineSession {
    @discardableResult
    func sendToolbarKey(_ action: String) -> Bool {
        sendDisplayInput(.key(action))
    }
}

extension HvfEngineSession {
    func append(_ event: BvAgentEvent) {
        shareRootNotice.consume(event)
        diskSyncNotice.consume(event)
        agentUpdateNotice.consume(event)
        fileDrops.consume(event)
        clipboardImageNotice.consume(event)
        agentSilenceNotice.consume(event)
        events.append(HvfGuestLinks.feedEvent(event))
        if events.count > 500 { events.removeFirst(events.count - 500) }
    }
}

extension HvfEngineSession {
    func finishLegacyPaste(lines: [String]) {
        guard let ready = legacyPaste.pending?.consume(lines: lines, now: Date()) else { return }
        let onPasteSent = legacyPaste.pending?.onPasteSent
        let backlog = legacyPaste.finish()
        if ready, case .connected = connectionState {
            if appendLiveInput("KEY ctrl+v") {
                onPasteSent?()
                replayDisplayInput(backlog)
            }
            else { append(.unknown("display input replay stopped" + HvfLegacyPasteState.discarded(backlog))) }
        } else { append(.unknown("clipboard paste failed, expired or canceled" + HvfLegacyPasteState.discarded(backlog))) }
    }
}

extension HvfEngineSession {
    // The injected launcher must run the supplied Process or throw before spawning it.
    convenience init(config: HvfEngineConfig, repoRoot: URL,
                     processLaunch: @escaping (Process) throws -> Void,
                     processIsRunning: @escaping (String) -> Bool,
                     vtpmKeyProvider: VTPMStateKeyProviding) {
        self.init(config: config, repoRoot: repoRoot,
                  processIsRunning: processIsRunning, vtpmKeyProvider: vtpmKeyProvider)
        self.processLaunch = processLaunch
    }
}

extension HvfEngineSession {
    @discardableResult
    func acceptStartConfiguration(_ config: HvfEngineConfig) -> Bool {
        guard !hasActiveRuntimeWork, !workAdmissionGate.isChecking else { return false }
        guard workAdmissionGate.check(workAdmission, reportRefusal: true) == nil else { return false }
        self.config = config
        return true
    }

    @discardableResult
    func attachIfStopped() -> Bool {
        guard !hasActiveRuntimeWork, !workAdmissionGate.isChecking else { return false }
        return attachToRunningVM(reportRefusal: false)
    }

}

extension HvfEngineSession {
    /// Caller has already checked the external runtime and holds a transition or start reservation.
    func adoptObservedAttachment(reportDuplicateLaunch: Bool) {
        timer?.invalidate(); timer = nil
        process = nil; ownedController = nil
        closeLiveInput()
        attachedToExistingProcess = true
        nextAttachedLivenessCheck = HvfAttachedLivenessSchedule.next(after: Date())
        resetObservedRuntimeState(clearEvents: true)
        connectionState = .booting
        startPolling() // reads run.log from the start
        resumeAfterReattach()
        if reportDuplicateLaunch {
            append(.unknown("attached to the already running HVF engine; duplicate launch prevented"))
        }
    }
}

/// Clipboard sync is two-way, so a library VM's choice is saved in vm.json (see
/// HvfLibraryPreferenceSave) and every start path launches with it.
extension HvfEngineSession {
    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyClipboardSyncToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsLibraryPreferences else { return (enabled, nil) }
        let message = saveClipboardSyncPreference(enabled, publish: publish)
        return (config.libraryContext?.config.clipboardSyncEnabled ?? true, message)
    }

    /// Returns nil once vm.json holds `enabled`, otherwise the refusal or warning.
    /// `true` is saved as an absent key so re-enabled VMs match never-touched ones.
    func saveClipboardSyncPreference(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        saveLibraryPreference("Share clipboard", publish: publish) { $0.clipboardSyncEnabled = enabled ? nil : false }
    }
}

extension HvfEngineSession {
    /// Asks a connected guest to shut down once a stop is pending, or ends the wrapper when it cannot.
    /// A legacy wrapper launch has no Force Stop control, so its 180 s deadline still ends the wrapper.
    func sendGracefulStopIfReady() {
        guard case .stopping = connectionState, serviceStarted, !stopCommandSent else { return }
        stopCommandSent = true
        if sendCtl(HvfGuestStopCommand.shutdown.line) {
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
}

/// A guest the user took offline stays offline on every start path: a library VM's
/// "Virtio net" choice is saved in vm.json (`networkEnabled`; absent means on).
extension HvfEngineSession {
    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyNetworkToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsLibraryPreferences else { return (enabled, nil) }
        let message = saveNetworkPreference(enabled, publish: publish)
        return (config.libraryContext?.config.networkEnabled ?? true, message)
    }

    /// Writes only when the effective value changes. VM creation saves an explicit `true`,
    /// so turning a created VM's network back on restores its original bytes.
    func saveNetworkPreference(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        saveLibraryPreference("Enable networking", publish: publish) {
            if ($0.networkEnabled ?? true) != enabled { $0.networkEnabled = enabled }
        }
    }
}

extension HvfEngineSession: ControlAppTerminationSession {
    /// The app's exit closes an owned runner's input, which cancels it; a pending start may still spawn one.
    /// A runner that already died has nothing left to cancel or confirm, so waiting for it only held
    /// Quit for two deadlines behind a shutdown question and a Force Stop that had no target.
    var blocksAppTermination: Bool { (mayHaveOwnedWork && !runnerDiedWithoutCleanup) || hasPendingRuntimeStart }
    var terminationName: String? { displayTitle }

    /// This app's own run, with no stop under way yet: the Stop below, and the Virtual Machine menu's, can begin.
    var terminationStopAvailable: Bool {
        guard let controller = ownedController, controller.operation == nil,
              let identity = ownedProcessIdentity else { return false }
        return identity == controller.identity
    }

    func requestTerminationStop() {
        guard terminationStopAvailable, let identity = ownedProcessIdentity else { return }
        stopOwned(expectedToken: identity.token)
    }

    func requestTerminationForceStop() {
        guard let target = forceStopTarget else { return }
        _ = requestForceStop(target)
    }

    func pollTerminationWait() { poll() }
}
