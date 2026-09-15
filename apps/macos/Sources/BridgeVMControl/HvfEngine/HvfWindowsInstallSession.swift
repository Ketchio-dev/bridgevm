import Foundation
import Combine

@MainActor
final class HvfWindowsInstallSession: ObservableObject {
    @Published private(set) var stage: HvfWindowsInstallStage = .idle
    @Published private(set) var logLines: [String] = []
    @Published private(set) var startedAt: Date?

    let plan: HvfWindowsInstallPlan
    var lifecycle = HvfWindowsInstallLifecycle()

    private let execution: HvfWindowsInstallExecution
    private var cancelled = false
    private let validate: HvfWindowsInstallValidationWorker.Validator
    private let schedule: (@escaping @MainActor () async -> Void) -> Void

    init(
        plan: HvfWindowsInstallPlan,
        validate: @escaping HvfWindowsInstallValidationWorker.Validator = { $0.validationError() },
        schedule: @escaping (@escaping @MainActor () async -> Void) -> Void = { work in Task { await work() } },
        execution: HvfWindowsInstallExecution? = nil
    ) {
        self.plan = plan
        self.validate = validate
        self.schedule = schedule
        self.execution = execution ?? HvfWindowsInstallExecution()
    }

    var isRunning: Bool { stage.isRunning }

    /// Acknowledges validation and pipeline scheduling, not installation completion.
    /// Use cancel(); cancelling this task handle does not cancel the install.
    @discardableResult
    func start() -> Task<Void, Never>? {
        guard !isRunning else { return nil }
        guard workAdmission?(true) == nil else { return nil }
        cancelled = false
        stage = .validating
        startedAt = Date()
        logLines = []
        execution.process.reset()
        return Task {
            guard !acknowledgeCancellation() else { return }
            let error = await HvfWindowsInstallValidationWorker.validate(plan, using: validate)
            guard !acknowledgeCancellation() else { return }
            if let error { stage = .failed(error); return }
            stage = .preparingSource
            schedule { await self.run() }
        }
    }

    func cancel() {
        guard isRunning, !cancelled else { return }
        cancelled = true
        stage = .cancelling
        execution.process.cancel()
        appendLog("사용자가 설치를 취소했습니다.")
    }

    private func run() async {
        guard !acknowledgeCancellation() else { return }
        let sourceLock: HvfWindowsInstallSourceLock
        do {
            sourceLock = try HvfWindowsInstallSourceLock(sourceImagePath: plan.sourceImagePath)
        } catch {
            stage = .failed(error.localizedDescription)
            return
        }
        defer { withExtendedLifetime(sourceLock) {} }
        let sourceVerified = await execution.verifySource(plan)
        guard !acknowledgeCancellation() else { return }
        if !sourceVerified {
            stage = .preparingSource
            let build = plan.sourceBuildCommand()
            guard await runProcess(arguments: build.arguments, extraEnvironment: build.environment,
                                   progressLog: nil) else {
                try? FileManager.default.removeItem(atPath: plan.sourceImagePath)
                failUnlessCancelled("설치 소스 생성이 실패했습니다.")
                return
            }
            guard !acknowledgeCancellation(cleanup: true) else { return }
        } else {
            appendLog("설치 소스 캐시 재사용: \(plan.sourceImagePath)")
        }

        stage = .installing
        do {
            try execution.prepareMedia(plan)
        } catch {
            failUnlessCancelled("번들된 UEFI vars 시드를 준비하지 못했습니다.")
            return
        }
        let installLog = URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log")
        guard await runProcess(arguments: plan.installCommand(), extraEnvironment: [:],
                               progressLog: installLog) else {
            failUnlessCancelled("Windows 무인 설치가 실패했습니다. 로그: \(plan.tmpEvidenceDir)/run.log")
            return
        }
        guard !acknowledgeCancellation(cleanup: true) else { return }

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
        if !acknowledgeCancellation(cleanup: true) { stage = .failed(message) }
    }

    private func acknowledgeCancellation(cleanup: Bool = false) -> Bool {
        guard cancelled else { return false }
        if cleanup { cleanupTemporaryMedia() }
        stage = .cancelled
        return true
    }

    private func cleanupTemporaryMedia() {
        let fm = FileManager.default
        for path in [plan.tmpTargetPath, plan.tmpVarsPath] {
            try? fm.removeItem(atPath: path)
        }
    }

    private func finalizeMedia() throws {
        try execution.finalize(plan)
        appendLog("UEFI 부팅 항목과 Microsoft-only Secure Boot 키를 검증·시드했습니다.")
    }

    private func runProcess(arguments: [String], extraEnvironment: [String: String], progressLog: URL?) async -> Bool {
        guard !cancelled else { return false }
        return await execution.process.run(plan: plan, arguments: arguments, environment: extraEnvironment,
            progressLog: progressLog, output: { [weak self] in self?.appendLog($0) })
    }

    private func appendLog(_ line: String) {
        logLines.append(line)
        if logLines.count > 400 {
            logLines.removeFirst(logLines.count - 400)
        }
    }
}
