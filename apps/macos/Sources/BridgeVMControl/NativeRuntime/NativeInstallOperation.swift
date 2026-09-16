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
        if let session { return session.isRunning }
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

    func fail(_ message: String) {
        guard terminal == nil, session == nil else { return }
        preparationTask = nil
        terminal = (.failed, bounded(message, limit: NativeInstallControlCodec.maximumFailureBytes), [])
    }

    @discardableResult
    func cancel() -> Bool {
        guard terminal == nil else { return false }
        if let session {
            guard session.canCancel else { return false }
            session.cancel()
            return true
        }
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
        switch session.stage {
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
        return make(phase: phase, worker: false, running: session.isRunning,
                    canCancel: session.canCancel, logs: boundedLogs(session.logLines), failure: failure)
    }

    private func make(phase: NativeInstallObservation.Phase, worker: Bool, running: Bool,
                      canCancel: Bool, logs: [String], failure: String?) -> NativeInstallObservation {
        .init(operationID: operationID.uuidString,
              expectedSavedConfigurationDigest: expectedSavedConfigurationDigest,
              phase: phase, acceptedUptime: acceptedUptime, workerPending: worker,
              sessionRunning: running, canCancel: canCancel, logTail: logs, failure: failure)
    }

    private func boundedLogs(_ input: [String]) -> [String] {
        var result: [String] = [], bytes = 0
        for line in input.suffix(NativeInstallControlCodec.maximumLogLines).reversed() {
            let remaining = NativeInstallControlCodec.maximumLogBytes - bytes
            guard remaining > 0 else { break }
            let value = bounded(line, limit: min(remaining, NativeInstallControlCodec.maximumLogLineBytes))
            guard !value.isEmpty else { break }
            result.append(value); bytes += value.utf8.count
        }
        return result.reversed()
    }

    private func bounded(_ value: String, limit: Int) -> String {
        guard value.utf8.count > limit else { return value }
        let prefix = Array(value.utf8.prefix(limit))
        for count in stride(from: prefix.count, through: 0, by: -1) {
            if let result = String(bytes: prefix.prefix(count), encoding: .utf8) { return result }
        }
        return ""
    }
}
