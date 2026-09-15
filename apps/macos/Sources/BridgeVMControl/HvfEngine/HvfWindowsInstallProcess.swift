import Foundation

@MainActor
final class HvfWindowsInstallProcess: HvfWindowsInstallProcessRunning {
    private var current: Process?
    private var logTimer: Timer?
    private var evidenceTail = TailOffsetReader()
    private var cancellationRequested = false
    private var activeID: UUID?

    func reset() { cancellationRequested = false }

    func cancel() {
        guard !cancellationRequested else { return }
        cancellationRequested = true
        if let current, current.isRunning { current.terminate() }
    }

    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment extraEnvironment: [String: String],
        progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        guard !cancellationRequested else { return false }
        let id = UUID()
        activeID = id
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = arguments
        process.currentDirectoryURL = plan.repoRoot
        var environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        environment["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
        for (key, value) in extraEnvironment { environment[key] = value }
        process.environment = environment
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        current = process
        output("$ \(arguments.joined(separator: " "))")
        startProgressTimer(progressLog, id: id, output: output)
        defer {
            logTimer?.invalidate()
            logTimer = nil
            activeID = nil
            current = nil
        }
        let accumulator = LineAccumulator()
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            let lines = accumulator.append(data)
            guard !lines.isEmpty else { return }
            Task { @MainActor [weak self] in
                guard self?.activeID == id else { return }
                for line in lines { output(line) }
            }
        }
        return await withCheckedContinuation { continuation in
            process.terminationHandler = { finished in
                pipe.fileHandleForReading.readabilityHandler = nil
                continuation.resume(returning: finished.terminationStatus == 0)
            }
            do { try process.run() }
            catch {
                pipe.fileHandleForReading.readabilityHandler = nil
                output("실행 실패: \(error.localizedDescription)")
                continuation.resume(returning: false)
            }
        }
    }

    private func startProgressTimer(_ progressLog: URL?, id: UUID, output: @escaping (String) -> Void) {
        guard let progressLog else { return }
        evidenceTail = TailOffsetReader()
        logTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self, self.activeID == id else { return }
                for line in self.evidenceTail.readNewLines(from: progressLog)
                    where HvfWindowsInstallSession.isProgressLine(line) { output(line) }
            }
        }
    }
}

extension HvfWindowsInstallSession {
    /// Keep only load-bearing boot lines out of the very chatty run.log.
    nonisolated static func isProgressLine(_ line: String) -> Bool {
        line.contains("BOOT_TIMER ramfb source=") && line.contains("state=captured")
            || line.hasPrefix("BVAGENT ")
            || line.contains("NVMe disk written back")
            || line.contains("stop: PSCI")
    }
}
