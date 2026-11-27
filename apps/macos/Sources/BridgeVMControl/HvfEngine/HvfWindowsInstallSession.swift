import Foundation
import Combine

@MainActor
final class HvfWindowsInstallSession: ObservableObject, ControlAppTerminationSession {
    @Published private(set) var stage: HvfWindowsInstallStage = .idle {
        didSet { if stage != oldValue { onStageChange?() } }
    }
    var onStageChange: (() -> Void)?
    @Published private(set) var logLines: [String] = []
    @Published private(set) var sourceActivity: String?
    @Published private(set) var startedAt: Date?

    /// The source builder found no Arm64 boot file on the ISO (an x64 Windows ISO).
    nonisolated static let notArm64ISO =
        "This ISO isn’t Windows 11 for Arm, so delete this VM and create a new one with the Arm64 ISO from Microsoft."

    let plan: HvfWindowsInstallPlan
    var lifecycle = HvfWindowsInstallLifecycle()
    let execution: HvfWindowsInstallExecution
    let validate: HvfWindowsInstallValidationWorker.Validator
    let schedule: (@escaping @MainActor () async -> Void) -> Void
    let recovery: HvfWindowsInstallRecoveryWorker
    var continuationAdmission: (@MainActor () -> String?)?
    var admissionPending = false
    /// An attempt runs for about half an hour, mostly unattended: like a running VM it keeps the Mac
    /// from idle sleep (the "Keep Mac awake" setting), which would freeze the build or Windows setup.
    var attemptActive = false { didSet { sleepPrevention.update(sleepPreventionID, active: attemptActive) } }
    let sleepPreventionID = UUID()
    var sleepPrevention = ControlAppPower.shared
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
    var blocksAppTermination: Bool { admissionPending || isRunning }
    var isInstallation: Bool { true }
    var requiresTerminationWait: Bool { isRunning && commitStarted }
    func requestTerminationStop() { if canCancel { cancel() } }
    func requestTerminationForceStop() { requestTerminationStop() }
    func pollTerminationWait() {}

    func cancel() {
        guard isRunning, !cancelled else { return }
        cancelled = true
        if commitStarted {
            appendLog("Finishing the installation. Your files will stay in place until it completes.")
            return
        }
        stage = .cancelling
        execution.process.cancel()
        appendLog("Installation cancelled by user.")
    }

    func prepareAttemptPresentation() {
        startedAt = Date()
        logLines = []
        sourceActivity = nil
        stage = cancelled ? .cancelling : .validating
    }

    func transition(to next: HvfWindowsInstallStage) { stage = next }

    func finish(_ terminal: HvfWindowsInstallStage) {
        stage = terminal
        attemptActive = false
    }

    func completeInstallation() {
        appendLog("Windows installation complete.")
        finish(.done)
        onCompleted?()
    }

    func failUnlessCancelled(_ message: String) {
        if !acknowledgeCancellation(cleanup: true) { cleanupTemporaryMedia(); finish(.failed(message)) }
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
            appendLog("Temporary files were kept to recover the interrupted installation.")
            return
        }
        try? HvfWindowsInstallStaging.discardMedia(plan)
    }

    func runProcess(arguments: [String], extraEnvironment: [String: String], progressLog: URL?) async -> Bool {
        guard !cancelled else { return false }
        return await execution.process.run(plan: plan, arguments: arguments, environment: extraEnvironment,
            progressLog: progressLog, output: { [weak self] in self?.appendLog($0) })
    }

    func appendLog(_ line: String) {
        if stage == .preparingSource, let activity = HvfWindowsInstallStage.sourceActivity(for: line) {
            sourceActivity = activity
        }
        logLines.append(line)
        if logLines.count > 400 { logLines.removeFirst(logLines.count - 400) }
    }
}

extension HvfWindowsInstallSession {
    func run() async {
        guard await continueFreshInstallation() else { return }
        let sourceLock: HvfWindowsInstallSourceLock
        do {
            sourceLock = try HvfWindowsInstallSourceLock(sourceImagePath: plan.sourceImagePath)
        } catch {
            appendLog(WindowsFileFailureFeedback.diagnostic("source-lock", error))
            finish(.failed(WindowsFileFailureFeedback.source))
            return
        }
        defer { withExtendedLifetime(sourceLock) {} }
        let sourceVerified = await execution.verifySource(plan)
        guard !acknowledgeCancellation() else { return }
        guard await continueFreshInstallation() else { return }
        if !sourceVerified {
            transition(to: .preparingSource)
            guard !acknowledgeCancellation() else { return }
            let build = plan.sourceBuildCommand()
            guard await runProcess(arguments: build.arguments, extraEnvironment: build.environment,
                                   progressLog: nil) else {
                try? FileManager.default.removeItem(atPath: plan.sourceImagePath)
                // The ISO is sealed into this VM, so Try Again cannot help an x64 one.
                let notArm = logLines.contains { $0.contains("FAIL: not-arm64-iso") }
                failUnlessCancelled(notArm ? Self.notArm64ISO : "Couldn’t prepare the setup files from the Windows ISO.")
                return
            }
            guard !acknowledgeCancellation(cleanup: true) else { return }
        } else {
            appendLog("Reusing cached installation media at \(plan.sourceImagePath).")
        }

        transition(to: .installing)
        guard !acknowledgeCancellation(cleanup: true) else { return }
        guard await continueFreshInstallation() else { return }
        do {
            try await execution.prepareMedia(plan)
        } catch {
            appendLog(WindowsFileFailureFeedback.diagnostic("preparation", error))
            failUnlessCancelled(WindowsFileFailureFeedback.preparation)
            return
        }
        guard !acknowledgeCancellation(cleanup: true) else { return }
        guard await continueFreshInstallation(cleanupOnCancellation: true) else { return }
        guard !acknowledgeCancellation(cleanup: true) else { return }
        if let refusal = continuationAdmission?() {
            failUnlessCancelled(refusal)
            return
        }
        let installLog = URL(fileURLWithPath: plan.stagingEvidenceDir).appendingPathComponent("run.log")
        guard await runProcess(arguments: plan.installCommand(), extraEnvironment: [:],
                               progressLog: installLog) else {
            failUnlessCancelled("Windows installation failed. See \(plan.stagingEvidenceDir)/run.log for details.")
            return
        }
        guard !acknowledgeCancellation(cleanup: true) else { return }
        guard await continueFreshInstallation() else { return }

        // Durable publication has no rollback-on-cancel promise, including reentrant observers.
        commitStarted = true
        transition(to: .finalizing)
        do {
            try await execution.finalize(plan)
            appendLog("Verified and seeded the UEFI boot entry and Microsoft-only Secure Boot keys.")
        } catch {
            appendLog(WindowsFileFailureFeedback.diagnostic("finalization", error))
            finish(.failed(WindowsFileFailureFeedback.finalization))
            return
        }
        completeInstallation()
    }
}

extension HvfWindowsInstallSession {
    /// Acknowledges inspection, validation and pipeline scheduling, not completion.
    /// Use cancel(); cancelling this task handle does not cancel the installation.
    @discardableResult
    func start(admit: () -> Bool = { true }) -> Task<Void, Never>? {
        guard !admissionPending, !isRunning, stage != .done else { return nil }
        // Existing library admission must not see its own attempt as a conflict.
        admissionPending = true
        defer { admissionPending = false }
        guard workAdmission?(true) == nil, admit() else { return nil }
        attemptActive = true
        cancelled = false
        commitStarted = false
        execution.process.reset()
        prepareAttemptPresentation()
        return Task { await inspectAndSchedule() }
    }

    private func inspectAndSchedule() async {
        guard !acknowledgeCancellation() else { return }
        do {
            let decision = try await recovery.decision(for: plan)
            guard !acknowledgeCancellation() else { return }
            if case let .pending(ticket) = decision {
                transition(to: .recovering)
                schedule { await self.recoverExisting(ticket) }
                return
            }
            let error = await HvfWindowsInstallValidationWorker.validate(plan, using: validate)
            guard !acknowledgeCancellation() else { return }
            if let error { finish(.failed(error)); return }
            transition(to: .preparingSource)
            schedule { await self.run() }
        } catch {
            if !acknowledgeCancellation() {
                appendLog(WindowsFileFailureFeedback.diagnostic("recovery-inspection", error))
                finish(.failed(WindowsFileFailureFeedback.recovery))
            }
        }
    }

    /// Recheck inside the scheduled pipeline and after awaits, before fresh effects.
    func continueFreshInstallation(cleanupOnCancellation: Bool = false) async -> Bool {
        guard !acknowledgeCancellation(cleanup: cleanupOnCancellation) else { return false }
        do {
            let decision = try await recovery.decision(for: plan)
            guard !acknowledgeCancellation(cleanup: cleanupOnCancellation) else { return false }
            switch decision {
            case .fresh: return true
            case let .pending(ticket):
                await recoverExisting(ticket)
                return false
            }
        } catch {
            if !acknowledgeCancellation(cleanup: cleanupOnCancellation) {
                appendLog(WindowsFileFailureFeedback.diagnostic("recovery-inspection", error))
                finish(.failed(WindowsFileFailureFeedback.recovery))
            }
            return false
        }
    }

    func recoverExisting(_ ticket: HvfWindowsInstallRecovery.Ticket) async {
        guard !acknowledgeCancellation() else { return }
        commitStarted = true
        transition(to: .recovering)
        appendLog("Recovering the previous Windows installation.")
        do {
            _ = try await recovery.resume(plan: plan, ticket: ticket)
            completeInstallation()
        } catch {
            appendLog(WindowsFileFailureFeedback.diagnostic("recovery", error))
            finish(.failed(WindowsFileFailureFeedback.recovery))
        }
    }
}
