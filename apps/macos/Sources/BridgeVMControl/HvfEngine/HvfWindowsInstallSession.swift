import Foundation
import Combine

@MainActor
final class HvfWindowsInstallSession: ObservableObject {
    @Published private(set) var stage: HvfWindowsInstallStage = .idle
    @Published private(set) var logLines: [String] = []
    @Published private(set) var startedAt: Date?

    let plan: HvfWindowsInstallPlan
    var lifecycle = HvfWindowsInstallLifecycle()
    let execution: HvfWindowsInstallExecution
    let validate: HvfWindowsInstallValidationWorker.Validator
    let schedule: (@escaping @MainActor () async -> Void) -> Void
    let recovery: HvfWindowsInstallRecoveryWorker
    var admissionPending = false
    var attemptActive = false
    var cancelled = false
    var commitStarted = false

    init(
        plan: HvfWindowsInstallPlan,
        validate: @escaping HvfWindowsInstallValidationWorker.Validator = { $0.validationError() },
        schedule: @escaping (@escaping @MainActor () async -> Void) -> Void = { work in Task { await work() } },
        execution: HvfWindowsInstallExecution? = nil,
        recovery: HvfWindowsInstallRecoveryWorker = .init()
    ) {
        self.plan = plan
        self.validate = validate
        self.schedule = schedule
        self.execution = execution ?? HvfWindowsInstallExecution()
        self.recovery = recovery
    }

    // The reservation is committed before Published callbacks and survives every await.
    var isRunning: Bool { attemptActive }
    var canCancel: Bool { isRunning && !cancelled && !commitStarted }

    func cancel() {
        guard isRunning, !cancelled else { return }
        cancelled = true
        if commitStarted {
            appendLog("설치 결과를 반영하고 있습니다. 파일을 보존하고 완료 결과를 기다립니다.")
            return
        }
        stage = .cancelling
        execution.process.cancel()
        appendLog("사용자가 설치를 취소했습니다.")
    }

    func prepareAttemptPresentation() {
        startedAt = Date()
        logLines = []
        stage = cancelled ? .cancelling : .validating
    }

    func transition(to next: HvfWindowsInstallStage) { stage = next }

    func finish(_ terminal: HvfWindowsInstallStage) {
        stage = terminal
        attemptActive = false
    }

    func completeInstallation() {
        appendLog("Windows 설치가 완료되었습니다.")
        finish(.done)
        onCompleted?()
    }

    func failUnlessCancelled(_ message: String) {
        if !acknowledgeCancellation(cleanup: true) { finish(.failed(message)) }
    }

    func acknowledgeCancellation(cleanup: Bool = false) -> Bool {
        guard cancelled, !commitStarted else { return false }
        if cleanup { cleanupTemporaryMedia() }
        finish(.cancelled)
        return true
    }

    private func cleanupTemporaryMedia() {
        // A journal, ambiguous path or failed inspection owns these inputs until recovery.
        guard (try? HvfWindowsInstallRecovery.inspect(plan: plan)) == .fresh else {
            appendLog("중단된 설치 복구에 필요한 임시 파일을 보존했습니다.")
            return
        }
        for path in [plan.tmpTargetPath, plan.tmpVarsPath] {
            try? FileManager.default.removeItem(atPath: path)
        }
    }

    func runProcess(arguments: [String], extraEnvironment: [String: String], progressLog: URL?) async -> Bool {
        guard !cancelled else { return false }
        return await execution.process.run(plan: plan, arguments: arguments, environment: extraEnvironment,
            progressLog: progressLog, output: { [weak self] in self?.appendLog($0) })
    }

    func appendLog(_ line: String) {
        logLines.append(line)
        if logLines.count > 400 { logLines.removeFirst(logLines.count - 400) }
    }
}
