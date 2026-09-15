import Foundation
import Combine

@MainActor
final class HvfWindowsInstallSession: ObservableObject {
    @Published private(set) var stage: HvfWindowsInstallStage = .idle
    @Published private(set) var logLines: [String] = []
    @Published private(set) var startedAt: Date?

    let plan: HvfWindowsInstallPlan
    /// Called after the transaction has durably published the completed config.
    var onCompleted: (() -> Void)?

    private var currentProcess: Process?
    private var logTimer: Timer?
    private var evidenceTail = TailOffsetReader()
    private var cancelled = false
    private let validate: (HvfWindowsInstallPlan) -> String?
    private let schedule: (@escaping @MainActor () async -> Void) -> Void

    init(
        plan: HvfWindowsInstallPlan,
        validate: @escaping (HvfWindowsInstallPlan) -> String? = { $0.validationError() },
        schedule: @escaping (@escaping @MainActor () async -> Void) -> Void = { work in Task { await work() } }
    ) {
        self.plan = plan
        self.validate = validate
        self.schedule = schedule
    }

    var isRunning: Bool {
        switch stage {
        case .preparingSource, .installing, .finalizing: return true
        default: return false
        }
    }

    func start() {
        guard !isRunning else { return }
        if let error = validate(plan) {
            stage = .failed(error)
            return
        }
        cancelled = false
        stage = .preparingSource
        startedAt = Date()
        logLines = []
        evidenceTail = TailOffsetReader()
        schedule { await self.run() }
    }

    func cancel() {
        cancelled = true
        currentProcess?.terminate()
        appendLog("사용자가 설치를 취소했습니다.")
    }

    private func run() async {
        guard !cancelled else {
            stage = .failed("설치가 취소되었습니다.")
            return
        }
        let sourceLock: HvfWindowsInstallSourceLock
        do {
            sourceLock = try HvfWindowsInstallSourceLock(sourceImagePath: plan.sourceImagePath)
        } catch {
            stage = .failed(error.localizedDescription)
            return
        }
        defer { withExtendedLifetime(sourceLock) {} }
        if !(await plan.sourceImageCacheIsVerified()) {
            stage = .preparingSource
            let build = plan.sourceBuildCommand()
            guard await runProcess(arguments: build.arguments, extraEnvironment: build.environment,
                                   progressLog: nil) else {
                try? FileManager.default.removeItem(atPath: plan.sourceImagePath)
                failUnlessCancelled("설치 소스 생성이 실패했습니다.")
                return
            }
        } else {
            appendLog("설치 소스 캐시 재사용: \(plan.sourceImagePath)")
        }

        stage = .installing
        do {
            try HvfWindowsBootSeed.writeBundledSeed(to: plan.tmpVarsPath)
        } catch {
            failUnlessCancelled("번들된 UEFI vars 시드를 준비하지 못했습니다.")
            return
        }
        try? FileManager.default.createDirectory(
            atPath: plan.tmpEvidenceDir, withIntermediateDirectories: true)
        let installLog = URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log")
        guard await runProcess(arguments: plan.installCommand(), extraEnvironment: [:],
                               progressLog: installLog) else {
            failUnlessCancelled("Windows 무인 설치가 실패했습니다. 로그: \(plan.tmpEvidenceDir)/run.log")
            return
        }

        stage = .finalizing
        do {
            try finalizeMedia()
        } catch {
            stage = .failed("설치 결과 반영 실패: \(error.localizedDescription)")
            return
        }
        stage = .done
        appendLog("Windows 설치가 완료되었습니다.")
        onCompleted?()
    }

    private func failUnlessCancelled(_ message: String) {
        if cancelled {
            cleanupTemporaryMedia()
            stage = .failed("설치가 취소되었습니다.")
        } else {
            stage = .failed(message)
        }
    }

    private func cleanupTemporaryMedia() {
        let fm = FileManager.default
        for path in [plan.tmpTargetPath, plan.tmpVarsPath] {
            try? fm.removeItem(atPath: path)
        }
    }

    private func finalizeMedia() throws {
        try HvfWindowsInstallFinalization.finalize(plan: plan)
        appendLog("UEFI 부팅 항목과 Microsoft-only Secure Boot 키를 검증·시드했습니다.")
    }

    /// Runs one pipeline Process off the main actor, streaming its stdout and
    /// optionally tailing a separate evidence log for boot progress lines.
    private func runProcess(
        arguments: [String],
        extraEnvironment: [String: String],
        progressLog: URL?
    ) async -> Bool {
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
        currentProcess = process

        appendLog("$ \(arguments.joined(separator: " "))")
        startProgressTimer(progressLog: progressLog)
        defer {
            stopProgressTimer()
            currentProcess = nil
        }

        let accumulator = LineAccumulator()
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            let lines = accumulator.append(data)
            guard !lines.isEmpty else { return }
            Task { @MainActor [weak self] in
                for line in lines { self?.appendLog(line) }
            }
        }

        return await withCheckedContinuation { continuation in
            process.terminationHandler = { finished in
                pipe.fileHandleForReading.readabilityHandler = nil
                let ok = finished.terminationStatus == 0
                continuation.resume(returning: ok)
            }
            do {
                try process.run()
            } catch {
                pipe.fileHandleForReading.readabilityHandler = nil
                Task { @MainActor [weak self] in
                    self?.appendLog("실행 실패: \(error.localizedDescription)")
                }
                continuation.resume(returning: false)
            }
        }
    }

    private func startProgressTimer(progressLog: URL?) {
        guard let progressLog else { return }
        evidenceTail = TailOffsetReader()
        logTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                let lines = self.evidenceTail.readNewLines(from: progressLog)
                for line in lines where Self.isProgressLine(line) {
                    self.appendLog(line)
                }
            }
        }
    }

    private func stopProgressTimer() {
        logTimer?.invalidate()
        logTimer = nil
    }

    /// Keep only load-bearing boot lines out of the very chatty run.log.
    nonisolated static func isProgressLine(_ line: String) -> Bool {
        line.contains("BOOT_TIMER ramfb source=") && line.contains("state=captured")
            || line.hasPrefix("BVAGENT ")
            || line.contains("NVMe disk written back")
            || line.contains("stop: PSCI")
    }

    private func appendLog(_ line: String) {
        logLines.append(line)
        if logLines.count > 400 {
            logLines.removeFirst(logLines.count - 400)
        }
    }
}
