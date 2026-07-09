import Foundation
import Combine
import Darwin
#if canImport(AppKit)
import AppKit
#endif

enum HvfConnectionState: Equatable {
    case stopped
    case booting
    case connected(host: String)
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

    init(config: HvfEngineConfig, repoRoot: URL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).deletingLastPathComponent().deletingLastPathComponent()) {
        self.config = config
        self.repoRoot = repoRoot
    }

    deinit {
        timer?.invalidate()
        process?.terminate()
    }

    func start() {
        stop()
        try? FileManager.default.createDirectory(atPath: config.evidenceDir, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(atPath: (config.ctlFilePath as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        proc.arguments = config.wrapperArguments()
        proc.currentDirectoryURL = repoRoot
        proc.environment = ProcessInfo.processInfo.environment.merging(config.environment()) { _, new in new }
        process = proc
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
        timer?.invalidate()
        timer = nil
        if let pid = process?.processIdentifier {
            _ = Darwin.kill(-pid, SIGTERM)
            process?.terminate()
        }
        process = nil
        connectionState = .stopped
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
        if let process, !process.isRunning, case .booting = connectionState {
            connectionState = .stopped
        }
    }

    private func handle(_ event: BvAgentEvent) {
        append(event)
        switch event {
        case let .ready(host, _):
            connectionState = .connected(host: host)
        case .aliveHeartbeat:
            lastHeartbeatDate = Date()
            lastHeartbeatAge = 0
        case .timeout:
            connectionState = .timedOut
        default:
            break
        }
    }

    private func append(_ event: BvAgentEvent) {
        events.append(event)
        if events.count > 500 {
            events.removeFirst(events.count - 500)
        }
    }

    private func refreshScreenshot() {
        #if canImport(AppKit)
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
