import Foundation
import Combine
#if os(macOS)
import Darwin
#endif
#if canImport(AppKit)
import AppKit
#endif

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
    #if canImport(AppKit)
    @Published var latestScreenshot: NSImage?
    #endif

    var repoRoot: URL
    private var process: Process?
    private var timer: Timer?
    private var tailReader = TailOffsetReader()
    private var lastHeartbeatDate: Date?
    private var serviceStarted = false
    private var stopCommandSent = false
    private var stopDeadline: Date?
    private var attachedToExistingProcess = false
    private var liveInputHandle: FileHandle?
    private var liveInputPath: URL?
    private var liveInputWriteFailureReported = false
    private var lastScreenshotFingerprint: HvfScreenshotFingerprint?
    private var injectionConfirmed = false
    private let processIsRunning: (String) -> Bool

    nonisolated static func defaultRepoRoot(
        currentDirectoryPath: String = FileManager.default.currentDirectoryPath,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        executablePath: String? = Bundle.main.executableURL?.path,
        resourcePath: String? = Bundle.main.resourceURL?.path
    ) -> URL {
        if let override = environment["BRIDGEVM_REPO_ROOT"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !override.isEmpty {
            let expanded = (override as NSString).expandingTildeInPath
            let url = URL(fileURLWithPath: expanded, isDirectory: true)
            if containsBootWrapper(url) { return url.resolvingSymlinksInPath() }
        }

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
        processIsRunning: @escaping (String) -> Bool = { Shell.isProcessRunning(matching: $0) }
    ) {
        self.config = config
        self.repoRoot = repoRoot
        self.processIsRunning = processIsRunning
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
        let wrapper = repoRoot.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        guard FileManager.default.isExecutableFile(atPath: wrapper.path) else {
            append(.unknown("launch failed: installed-boot wrapper not found at \(wrapper.path)"))
            connectionState = .stopped
            return
        }
        tailReader = TailOffsetReader()
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        stopCommandSent = false
        stopDeadline = nil
        events = []
        injectionConfirmed = false
        liveInputWriteFailureReported = false
        #if canImport(AppKit)
        latestScreenshot = nil
        lastScreenshotFingerprint = nil
        #endif
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        proc.arguments = config.wrapperArguments()
        proc.currentDirectoryURL = repoRoot
        proc.environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        process = proc
        attachedToExistingProcess = false
        connectionState = .booting
        do {
            try proc.run()
            startPolling()
        } catch {
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
        let ascii = value.utf8.filter { (0x20...0x7e).contains($0) }
        for chunkStart in stride(from: 0, to: ascii.count, by: 32) {
            let chunkEnd = min(chunkStart + 32, ascii.count)
            let encoded = ascii[chunkStart..<chunkEnd]
                .map { String(format: "%02x", $0) }
                .joined()
            appendLiveInput("KEY text-hex:\(encoded)")
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
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
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
        for name in ["display.ppm", "display.ppm.tmp", "input.ctl", "run.log"] {
            let url = evidenceDirectory.appendingPathComponent(name)
            if fileManager.fileExists(atPath: url.path) {
                try fileManager.removeItem(at: url)
            }
        }
        try Data().write(to: evidenceDirectory.appendingPathComponent("input.ctl"))

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
            checkInjectionProgress(lines)
        }
        if let lastHeartbeatDate {
            lastHeartbeatAge = Date().timeIntervalSince(lastHeartbeatDate)
        }
        refreshScreenshot()
        if let process, !process.isRunning {
            markStopped()
            return
        }
        if attachedToExistingProcess && !processIsRunning(config.targetDiskPath) {
            markStopped()
            return
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

    /// The injection boot confirms driver activation the moment the display
    /// pipeline switches from ramfb to the 3D scanout: the wrapper's
    /// BOOT_TIMER line reports `source=virtio-gpu ... state=captured` only
    /// when viogpu3d is bound and presenting. The pending marker is then
    /// retired so later boots run without the injector disk.
    private func checkInjectionProgress(_ lines: [String]) {
        guard !injectionConfirmed, let markerPath = config.injectPendingMarkerPath else { return }
        guard lines.contains(where: {
            $0.contains("source=virtio-gpu") && $0.contains("state=captured")
        }) else { return }
        injectionConfirmed = true
        let doneName = (HvfWindowsInstallPlan.injectDoneMarker as NSString).lastPathComponent
        let donePath = (markerPath as NSString).deletingLastPathComponent + "/" + doneName
        let fileManager = FileManager.default
        try? fileManager.removeItem(atPath: donePath)
        do {
            try fileManager.moveItem(atPath: markerPath, toPath: donePath)
            append(.unknown("viogpu3d 3D 디스플레이 활성 확인 — 다음 부팅부터 인젝터 없이 시작합니다"))
        } catch {
            append(.unknown("3D 활성은 확인했지만 주입 마커 정리에 실패했습니다: \(error.localizedDescription)"))
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
        injectionConfirmed = false
        tailReader = TailOffsetReader()
        lastHeartbeatDate = nil
        lastHeartbeatAge = nil
        serviceStarted = false
        stopCommandSent = false
        stopDeadline = nil
        liveInputWriteFailureReported = false
        if clearEvents { events = [] }
        #if canImport(AppKit)
        latestScreenshot = nil
        lastScreenshotFingerprint = nil
        #endif
    }

    private func append(_ event: BvAgentEvent) {
        events.append(event)
        if events.count > 500 {
            events.removeFirst(events.count - 500)
        }
    }

    private func refreshScreenshot() {
        #if canImport(AppKit)
        let evidenceDirectory = URL(fileURLWithPath: config.evidenceDir, isDirectory: true)
        guard let (url, fingerprint) = HvfScreenshotSource.resolve(in: evidenceDirectory),
              fingerprint != lastScreenshotFingerprint else { return }
        if let image = PpmDecoder.decodeImage(at: url) {
            latestScreenshot = image
            lastScreenshotFingerprint = fingerprint
        }
        #endif
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
