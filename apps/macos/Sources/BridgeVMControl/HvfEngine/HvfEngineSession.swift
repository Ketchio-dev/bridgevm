import Foundation
import Combine
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
        try? FileManager.default.createDirectory(atPath: config.evidenceDir, withIntermediateDirectories: true)
        for name in ["display.ppm", "display.ppm.tmp", "input.ctl"] {
            try? FileManager.default.removeItem(
                atPath: URL(fileURLWithPath: config.evidenceDir).appendingPathComponent(name).path
            )
        }
        FileManager.default.createFile(
            atPath: URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("input.ctl").path,
            contents: nil
        )
        try? FileManager.default.createDirectory(atPath: (config.ctlFilePath as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: config.ctlFilePath) {
            FileManager.default.createFile(atPath: config.ctlFilePath, contents: nil)
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
        #if canImport(AppKit)
        latestScreenshot = nil
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
        attachedToExistingProcess = true
        resetObservedRuntimeState(clearEvents: true)
        connectionState = .booting
        startPolling()
        return true
    }

    func sendCtl(_ line: String) {
        let cleaned = line.trimmingCharacters(in: .newlines)
        guard !cleaned.isEmpty else { return }
        let path = config.ctlFilePath
        try? FileManager.default.createDirectory(atPath: (path as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: path) {
            FileManager.default.createFile(atPath: path, contents: nil)
        }
        guard let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: path)) else { return }
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        if let data = "\(cleaned)\n".data(using: .utf8) {
            try? handle.write(contentsOf: data)
        }
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
        guard let handle = try? FileHandle(forWritingTo: path) else { return }
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        if let data = "\(line)\n".data(using: .utf8) {
            try? handle.write(contentsOf: data)
        }
    }

    private func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
        poll()
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
        sendCtl("shutdown.exe /p /f")
        stopCommandSent = true
        append(.unknown("graceful guest shutdown requested"))
    }

    private func markStopped() {
        timer?.invalidate()
        timer = nil
        process = nil
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
        if clearEvents { events = [] }
        #if canImport(AppKit)
        latestScreenshot = nil
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
        let live = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("display.ppm")
        if let image = PpmDecoder.decodeImage(at: live) {
            latestScreenshot = image
            return
        }
        let dir = URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("ramfb", isDirectory: true)
        guard let urls = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.contentModificationDateKey]) else { return }
        let newest = urls
            .filter { $0.pathExtension.lowercased() == "ppm" }
            .max {
                ((try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast)
                    < ((try? $1.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast)
            }
        if let newest, let image = PpmDecoder.decodeImage(at: newest) {
            latestScreenshot = image
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
