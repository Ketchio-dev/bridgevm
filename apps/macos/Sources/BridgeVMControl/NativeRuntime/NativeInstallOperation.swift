import Foundation

@MainActor
final class NativeInstallOperation {
    let operationID: UUID
    let expectedSavedConfigurationDigest: String
    let acceptedUptime: Double
    private var preparationTask: Task<Void, Never>?
    private var session: HvfWindowsInstallSession?
    private var terminal: (NativeInstallObservation.Phase, String?, [String])?

    init(operationID: UUID, expectedSavedConfigurationDigest: String,
         acceptedUptime: Double = ProcessInfo.processInfo.systemUptime) {
        self.operationID = operationID
        self.expectedSavedConfigurationDigest = expectedSavedConfigurationDigest
        self.acceptedUptime = acceptedUptime
    }

    var reservesWork: Bool {
        if terminal != nil { return false }
        if let session { return session.stage == .idle || session.admissionPending || session.isRunning }
        return true
    }
    var cancelledBeforeSession: Bool {
        if case .cancelled? = terminal?.0 { return session == nil }
        return false
    }

    func attach(_ task: Task<Void, Never>) {
        guard terminal == nil, session == nil, preparationTask == nil else { task.cancel(); return }
        preparationTask = task
    }

    func adopt(_ session: HvfWindowsInstallSession) {
        guard terminal == nil, self.session == nil, session.stage != .idle else { return }
        preparationTask = nil
        self.session = session
    }

    func start(_ session: HvfWindowsInstallSession) -> Bool {
        guard terminal == nil, self.session == nil else { return false }
        // start publishes synchronously; cancellation must already have an owner.
        preparationTask = nil
        self.session = session
        if session.start(admit: { self.terminal == nil }) != nil { return true }
        self.session = nil
        return false
    }

    func fail(_ message: String) {
        guard terminal == nil, session == nil else { return }
        preparationTask = nil
        terminal = (.failed, bounded(message, limit: NativeInstallControlCodec.maximumFailureBytes), [])
    }

    @discardableResult
    func cancel() -> Bool {
        guard terminal == nil else { return false }
        if let session, !session.admissionPending || session.isRunning {
            guard session.canCancel else { return false }
            session.cancel()
            return true
        }
        session = nil
        preparationTask?.cancel()
        preparationTask = nil
        terminal = (.cancelled, nil, [])
        return true
    }

    func observation() throws -> NativeInstallObservation {
        let value: NativeInstallObservation
        if let terminal {
            value = make(phase: terminal.0, worker: false, running: false,
                         canCancel: false, logs: terminal.2, failure: terminal.1)
        } else if let session {
            value = make(session)
        } else {
            value = make(phase: .preparingPlan, worker: true, running: false,
                         canCancel: true, logs: [], failure: nil)
        }
        try NativeInstallControlCodec.validate(value)
        return value
    }

    private func make(_ session: HvfWindowsInstallSession) -> NativeInstallObservation {
        let phase: NativeInstallObservation.Phase
        var failure: String?
        // Published sends before storing the new stage, including on retries.
        let stage: HvfWindowsInstallStage
        if session.isRunning && session.cancelled && !session.commitStarted { stage = .cancelling }
        else if session.admissionPending { stage = session.isRunning ? .validating : .idle }
        else { stage = session.stage }
        switch stage {
        case .idle: phase = .preparingPlan
        case .validating: phase = .validating
        case .preparingSource: phase = .preparingSource
        case .installing: phase = .installing
        case .finalizing: phase = .finalizing
        case .recovering: phase = .recovering
        case .cancelling: phase = .cancelling
        case .cancelled: phase = .cancelled
        case .done: phase = .done
        case let .failed(message): phase = .failed; failure = bounded(message,
            limit: NativeInstallControlCodec.maximumFailureBytes)
        }
        return make(phase: phase, worker: phase == .preparingPlan, running: session.isRunning,
                    canCancel: phase == .preparingPlan || session.canCancel,
                    logs: boundedLogs(session.logLines), failure: failure)
    }

    private func make(phase: NativeInstallObservation.Phase, worker: Bool, running: Bool,
                      canCancel: Bool, logs: [String], failure: String?) -> NativeInstallObservation {
        .init(operationID: operationID.uuidString,
              expectedSavedConfigurationDigest: expectedSavedConfigurationDigest,
              phase: phase, acceptedUptime: acceptedUptime, workerPending: worker,
              sessionRunning: running, canCancel: canCancel, logTail: logs, failure: failure)
    }
}
