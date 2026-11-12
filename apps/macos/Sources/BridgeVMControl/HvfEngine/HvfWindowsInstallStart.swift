import Foundation

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
        appendLog("기존 Windows 설치 결과를 복구하고 있습니다.")
        do {
            _ = try await recovery.resume(plan: plan, ticket: ticket)
            completeInstallation()
        } catch {
            appendLog(WindowsFileFailureFeedback.diagnostic("recovery", error))
            finish(.failed(WindowsFileFailureFeedback.recovery))
        }
    }
}
