import Foundation
import Combine
#if os(macOS)
import Darwin
#endif
#if canImport(AppKit)
import AppKit
#endif

/// How often an attached session may ask the operating system whether the VM
/// it attached to is still alive.
///
/// A session that launched the VM holds a `Process` and can check for free. A
/// session that attached to an already-running VM has no handle, so the only
/// answer comes from `pgrep`, which measures about 24 ms here. Running that on
/// every 100 ms poll would spend roughly a quarter of a core on it; once per
/// second is about 2 percent.
enum HvfAttachedLivenessSchedule {
    static let interval: TimeInterval = 1

    static func isDue(now: Date, next: Date) -> Bool { now >= next }

    static func next(after now: Date) -> Date { now.addingTimeInterval(interval) }
}

enum HvfConnectionState: Equatable {
    case stopped
    case booting
    case connected(host: String)
    case stopping
    case timedOut
}

@MainActor
final class HvfEngineSession: ObservableObject {
    @Published var config: HvfEngineConfig
    @Published var connectionState: HvfConnectionState = .stopped
    @Published var lastHeartbeatAge: TimeInterval?
    @Published var events: [BvAgentEvent] = []
    var repoRoot: URL
    private var process: Process?
    private var timer: Timer?
    private var tailReader = TailOffsetReader()
    private var lastHeartbeatDate: Date?
    private var serviceStarted = false
    private var stopCommandSent = false
    private var stopDeadline: Date?
    private var attachedToExistingProcess = false
    private var nextAttachedLivenessCheck = Date.distantPast
    private var liveInputHandle: FileHandle?
    private var liveInputPath: URL?
    private var liveInputWriteFailureReported = false
    private let processIsRunning: (String) -> Bool
    private let vtpmKeyProvider: VTPMStateKeyProviding

    nonisolated static func defaultRepoRoot(
        currentDirectoryPath: String = FileManager.default.currentDirectoryPath,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        executablePath: String? = Bundle.main.executableURL?.path,
        resourcePath: String? = Bundle.main.resourceURL?.path
    ) -> URL {
        // DEBUG only: this points the app at a checkout and makes it run that
        // checkout's shell scripts. In a signed release build an environment
        // variable must not be able to choose what code the app executes.
        #if DEBUG
        if let override = environment["BRIDGEVM_REPO_ROOT"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !override.isEmpty {
            let expanded = (override as NSString).expandingTildeInPath
            let url = URL(fileURLWithPath: expanded, isDirectory: true)
            if containsBootWrapper(url) { return url.resolvingSymlinksInPath() }
        }
        #endif

        var candidates = [URL(fileURLWithPath: currentDirectoryPath, isDirectory: true)]
        if let executablePath {
            candidates.append(URL(fileURLWithPath: executablePath).deletingLastPathComponent())
        }
        if let resourcePath {
            candidates.append(URL(fileURLWithPath: resourcePath, isDirectory: true))
        }
        for candidate in candidates {
            if let root = repositoryRoot(startingAt: candidate) { return root }
        }
        return URL(fileURLWithPath: currentDirectoryPath, isDirectory: true).standardizedFileURL
    }

    private nonisolated static func containsBootWrapper(_ root: URL) -> Bool {
        FileManager.default.isExecutableFile(
            atPath: root.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh").path
        )
    }

    private nonisolated static func repositoryRoot(startingAt start: URL) -> URL? {
        var candidate = start.standardizedFileURL
        while true {
            if containsBootWrapper(candidate) { return candidate.resolvingSymlinksInPath() }
            let parent = candidate.deletingLastPathComponent()
            if parent.path == candidate.path { return nil }
            candidate = parent
        }
    }

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
    }

    deinit {
        timer?.invalidate()
        process?.terminate()
        try? liveInputHandle?.close()
    }

    func start() {
        guard process?.isRunning != true else {
            append(.unknown("launch ignored: HVF engine is already running"))
            return
        }
        if attachToRunningVM() {
            append(.unknown("attached to the already running HVF engine; duplicate launch prevented"))
            return
        }
        let readiness = config.readiness(repoRoot: repoRoot)
        guard readiness.launchReady else {
            for blocker in readiness.launchBlockers {
                append(.unknown("launch readiness blocked [\(blocker.code)]: \(blocker.summary)"))
            }
            connectionState = .stopped
            return
        }
        timer?.invalidate()
        timer = nil
        process = nil
        closeLiveInput()
        do {
            try prepareRuntimeFiles()
        } catch {
            append(.unknown("launch failed: unable to prepare HVF runtime files: \(error.localizedDescription)"))
            connectionState = .stopped
            return
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
                return
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
            return
        }
        process = proc
        attachedToExistingProcess = false
        connectionState = .booting
        do {
            try proc.run()
            do {
                try vtpmKeyInput?.deliverAfterLaunch()
            } catch {
                proc.terminate()
                append(.unknown("launch failed: unable to deliver the vTPM state key: \(error.localizedDescription)"))
                connectionState = .stopped
                process = nil
                return
            }
            startPolling()
        } catch {
            vtpmKeyInput?.discard()
            append(.unknown("launch failed: \(error.localizedDescription)"))
            connectionState = .stopped
            process = nil
        }
    }

    func stop() {
        let ownsRunningProcess = process?.isRunning == true
        let attachedProcessIsRunning = attachedToExistingProcess && processIsRunning(config.targetDiskPath)
        guard ownsRunningProcess || attachedProcessIsRunning else {
            markStopped()
            return
        }
        connectionState = .stopping
        stopDeadline = Date().addingTimeInterval(180)
        sendGracefulStopIfReady()
        if timer == nil { startPolling() }
    }

    @discardableResult
    func attachToRunningVM() -> Bool {
        guard process?.isRunning != true else { return false }
        guard processIsRunning(config.targetDiskPath) else { return false }
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
        appendLiveInput("KEY \(action)")
    }

    func sendText(_ value: String) {
        // A USB HID keyboard sends physical key usages, not characters, so
        // there is no usage code for "한" and the HID path can only ever carry
        // printable ASCII. Silently dropping the rest (which this did) makes
        // non-ASCII input look broken with no explanation, so split the two
        // cases and route non-ASCII through the guest clipboard, which is
        // base64(UTF-8) end to end (bvagent.ps1:377).
        let plan = HvfTextInputPlan.make(for: value)
        for chunk in plan.hidChunks {
            appendLiveInput("KEY text-hex:\(chunk)")
        }
        if let base64 = plan.clipboardBase64 {
            guard sendCtl("CLIPSET \(base64)") else { return }
            appendLiveInput("KEY ctrl+v")
        }
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
        appendLiveInput("POINTER scroll:\(delta)@\(point.x)x\(point.y)")
    }

    private func sendPointerAction(_ action: String, location: CGPoint, viewSize: CGSize, imageSize: CGSize) {
        guard let point = mappedPointer(location, viewSize: viewSize, imageSize: imageSize) else { return }
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

    private func prepareRuntimeFiles() throws {
        let fileManager = FileManager.default
        let evidenceDirectory = URL(fileURLWithPath: config.evidenceDir, isDirectory: true)
        try fileManager.createDirectory(
            at: evidenceDirectory,
            withIntermediateDirectories: true
        )
        // run.log is removed too: the wrapper recreates it, and a stale log
        // would otherwise replay old BVAGENT/BOOT_TIMER lines into this
        // session (false attach, false 3D-injection confirmation).
        for name in [
            "display.ppm", "display.ppm.tmp", "display.fb", "display.fb.tmp",
            "display.fb.iosurface", "input.ctl", "run.log"
        ] {
            let url = evidenceDirectory.appendingPathComponent(name)
            if fileManager.fileExists(atPath: url.path) {
                try fileManager.removeItem(at: url)
            }
        }
        try Data().write(to: evidenceDirectory.appendingPathComponent("input.ctl"))
        // The versioned manifest this launch means, written where the run's
        // evidence lives. The wrapper does not read it yet; materializing it
        // per launch makes every session auditable against the runtime
        // contract (`hvf-runner --launch-spec launch-manifest.json` must
        // accept it) before the runtime owns the launch itself.
        try Data(config.launchManifestJSON().utf8)
            .write(to: evidenceDirectory.appendingPathComponent("launch-manifest.json"))

        let controlURL = URL(fileURLWithPath: config.ctlFilePath)
        try fileManager.createDirectory(
            at: controlURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        if fileManager.fileExists(atPath: controlURL.path) {
            let attributes = try fileManager.attributesOfItem(atPath: controlURL.path)
            guard attributes[.type] as? FileAttributeType == .typeRegular else {
                throw NSError(
                    domain: "BridgeVM.HvfEngineSession",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "control path is not a regular file: \(controlURL.path)"]
                )
            }
        } else {
            try Data().write(to: controlURL)
        }
    }

    private func poll() {
        let logURL = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("run.log")
        let lines = tailReader.readNewLines(from: logURL)
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

}

#if canImport(AppKit)
enum HvfDisplayCoordinates {
    static func absolutePointer(
        location: CGPoint,
        viewSize: CGSize,
        imageSize: CGSize
    ) -> (x: UInt16, y: UInt16)? {
        guard viewSize.width > 0, viewSize.height > 0,
              imageSize.width > 0, imageSize.height > 0 else { return nil }
        let scale = min(viewSize.width / imageSize.width, viewSize.height / imageSize.height)
        let displayed = CGSize(width: imageSize.width * scale, height: imageSize.height * scale)
        let origin = CGPoint(
            x: (viewSize.width - displayed.width) / 2,
            y: (viewSize.height - displayed.height) / 2
        )
        guard location.x >= origin.x, location.y >= origin.y,
              location.x <= origin.x + displayed.width,
              location.y <= origin.y + displayed.height else { return nil }
        let x = ((location.x - origin.x) / displayed.width * 32_767).rounded()
        let y = ((location.y - origin.y) / displayed.height * 32_767).rounded()
        return (UInt16(clamping: Int(x)), UInt16(clamping: Int(y)))
    }
}
#endif
