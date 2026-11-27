import Darwin
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
        let handle = pipe.fileHandleForReading
        let accumulator = LineAccumulator()
        let source = DispatchSource.makeReadSource(fileDescriptor: handle.fileDescriptor, queue: .main)
        // Only the main queue reads this pipe. On Darwin st_size is the number
        // of queued pipe bytes; a descendant may keep its write end open.
        func drain() {
            var info = stat()
            guard fstat(handle.fileDescriptor, &info) == 0 else { return }
            var remaining = Int(info.st_size)
            while remaining > 0, let data = try? handle.read(upToCount: remaining), !data.isEmpty {
                for line in accumulator.append(data) { output(line) }
                remaining -= data.count
            }
        }
        source.setEventHandler { [weak self] in
            guard self?.activeID == id else { return }
            drain()
            if source.data == 0 { source.cancel() }
        }
        source.resume()
        let succeeded = await withCheckedContinuation { continuation in
            process.terminationHandler = { finished in
                continuation.resume(returning: finished.terminationStatus == 0)
            }
            do { try process.run() }
            catch {
                output("Couldn’t start the installation command. \(error.localizedDescription)")
                continuation.resume(returning: false)
            }
        }
        source.cancel()
        drain()
        // The last diagnostic need not end in a newline.
        for line in accumulator.append(Data([0x0a])) { output(line) }
        return succeeded
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
            || line.hasPrefix("NVMe disk written back")
            || line.hasPrefix("BVSTORAGE full") // the VM paused on a full Mac disk (nvme/disk_full.rs)
            || line.contains("stop: PSCI")
    }
}
