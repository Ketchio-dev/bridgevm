import Foundation

@MainActor
final class HvfOwnedRunController {
    static let guestGrace: TimeInterval = 180
    static let completionObservation: TimeInterval = 12
    let identity: HvfOwnedRuntimeIdentity
    let config: HvfEngineConfig
    let process: Process
    private(set) var ledger: HvfOwnedRuntimeLedger
    private(set) var runnerExit: HvfOwnedRuntimeExit?
    private(set) var failure: HvfOwnedStopFailure?
    private(set) var operation: HvfOwnedStopOperation?
    private let channel: HvfOwnedRuntimeChannel
    private let guestShutdown: HvfOwnedGuestShutdown
    private let now: () -> TimeInterval
    private var eventTask: Task<Void, Never>?
    private var graceDeadline: TimeInterval?
    private var cancelled = false
    private var serviceReady = false
    private var guestRequestSent = false
    private var guestEffectPending = false
    private var outputEnded = false
    private var protocolValid = true
    private var teardownConfirmedAt: TimeInterval?
    var onChange: (() -> Void)?
    var onDiagnostic: ((String) -> Void)?
    var teardownConfirmed: Bool { protocolValid && outputEnded && !guestEffectPending && ledger.complete != nil && runnerExit != nil }
    var mayHaveOwnedWork: Bool { !teardownConfirmed }

    init(identity: HvfOwnedRuntimeIdentity, config: HvfEngineConfig, process: Process,
         manifestSHA256: String, channel: HvfOwnedRuntimeChannel,
         guestShutdown: HvfOwnedGuestShutdown,
         now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }) {
        self.identity = identity; self.config = config; self.process = process
        ledger = .init(identity: identity, manifestSHA256: manifestSHA256)
        self.channel = channel; self.guestShutdown = guestShutdown; self.now = now
    }

    func activate(helloFrame: Data) {
        let events = channel.events
        eventTask = Task { [weak self] in
            for await event in events {
                guard let self else { return }
                self.receive(event)
            }
        }
        process.terminationHandler = { [weak self] _ in
            Task { @MainActor [weak self] in self?.tick() }
        }
        channel.start(helloFrame: helloFrame)
    }

    func requestStop(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopAdmission {
        guard target == identity else { return .refused(.targetMismatch) }
        if let operation { return .existing(operation) }
        let time = now()
        let useGrace = ledger.ready && ledger.complete == nil && failure == nil && runnerExit == nil
        let phase: HvfOwnedStopPhase = useGrace ? .guestGrace : .cancelling
        let ticket = HvfOwnedStopOperation(operationID: operationID, target: identity, now: time,
            deadline: time + (useGrace ? Self.guestGrace : 0) + Self.completionObservation, phase: phase)
        operation = ticket // Publish admission before invoking any effect or callback.
        do { try ledger.admitStop(operationID: operationID) }
        catch { recordFailure(.protocolInvalid) }
        if ledger.complete != nil { refresh() }
        else if useGrace {
            graceDeadline = time + Self.guestGrace
            requestGuestIfReady()
        } else { cancelRunner() }
        tick()
        return .accepted(ticket)
    }

    func observation(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopObservation? {
        guard target == identity, operation?.operationID == operationID else { return nil }
        return operation?.observation
    }

    func guestServiceReady() {
        serviceReady = true
        requestGuestIfReady()
    }

    func tick() {
        if runnerExit == nil, !process.isRunning {
            runnerExit = HvfOwnedRuntimeExit(identity: identity, process: process)
            guestShutdown.close()
        }
        if let deadline = graceDeadline, now() >= deadline, ledger.complete == nil { cancelRunner() }
        let observedAt = now()
        if teardownConfirmed, teardownConfirmedAt == nil { teardownConfirmedAt = observedAt }
        if let ticket = operation, (teardownConfirmedAt ?? observedAt) >= ticket.observation.deadlineUptime {
            recordFailure(.deadlineExceeded)
        }
        if outputEnded, ledger.complete == nil, runnerExit != nil { recordFailure(.runnerExitedWithoutComplete) }
        refresh()
        onChange?()
    }

    func receive(_ event: HvfOwnedRuntimeChannelEvent) {
        switch event {
        case let .message(message):
            do {
                try ledger.accept(message)
                if message.kind == "cleanupUnconfirmed" { recordFailure(.cleanupUnconfirmed) }
                if ledger.complete != nil { graceDeadline = nil; guestShutdown.close() }
            } catch {
                protocolValid = false; recordFailure(.protocolInvalid); channel.close(); guestShutdown.close()
                onDiagnostic?("Owned runtime protocol rejected; cleanup remains unconfirmed")
            }
        case .eof:
            outputEnded = true
        case .failed:
            outputEnded = true; protocolValid = false; recordFailure(.channelFailed)
            onDiagnostic?("Owned runtime channel failed; cleanup remains unconfirmed")
        }
        tick()
    }

    private func requestGuestIfReady() {
        guard operation != nil, graceDeadline != nil, serviceReady, !guestRequestSent,
              runnerExit == nil, ledger.complete == nil else { return }
        guestRequestSent = true; guestEffectPending = true
        let token = identity.token
        guestShutdown.request { [weak self] delivered in
            Task { @MainActor [weak self] in
                guard let self, self.identity.token == token else { return }
                self.guestEffectPending = false
                guard self.graceDeadline != nil, self.runnerExit == nil, self.ledger.complete == nil else {
                    self.tick(); return
                }
                if delivered { self.onDiagnostic?("Guest shutdown command delivered; waiting for supervised cleanup") }
                else {
                    self.onDiagnostic?("Guest control ownership unavailable; requesting owned runtime cancellation")
                    self.cancelRunner()
                }
                self.tick()
            }
        }
    }

    private func cancelRunner() {
        guard !cancelled, ledger.complete == nil, let operation else { return }
        cancelled = true; graceDeadline = nil; guestShutdown.close()
        let deadline = min(operation.observation.deadlineUptime, now() + Self.completionObservation)
        operation.update(phase: .cancelling, deadline: deadline, complete: ledger.complete,
                         exit: runnerExit, failure: failure)
        do {
            try channel.sendStop(.init(schemaVersion: 1, kind: "stop",
                runToken: identity.token.uuidString.lowercased(), sequence: 1,
                operationID: operation.operationID.uuidString.lowercased()))
        } catch HvfOwnedRuntimeProtocolError.channelClosed {
            // The worker may already have queued a valid COMPLETE and EOF. Drain before deciding.
        } catch { recordFailure(.channelFailed); channel.close() }
    }

    private func recordFailure(_ value: HvfOwnedStopFailure) {
        // A later cleanup receipt cannot rewrite an earlier missed deadline or invalid transport.
        if failure == nil { failure = value }
    }

    private func refresh() {
        guard let operation else { return }
        if let complete = ledger.complete, complete.failureCode != nil { recordFailure(.cleanupFailed) }
        let phase: HvfOwnedStopPhase
        if failure != nil { phase = .unconfirmed }
        else if teardownConfirmed { phase = .completed }
        else if ledger.complete != nil { phase = .awaitingRunnerExit }
        else if graceDeadline != nil { phase = .guestGrace }
        else { phase = .cancelling }
        operation.update(phase: phase, complete: ledger.complete, exit: runnerExit, failure: failure)
        if teardownConfirmed { channel.close(); guestShutdown.close() }
    }

    func closeOwnerChannel() { channel.close(); guestShutdown.close() }
    deinit { eventTask?.cancel(); channel.close(); guestShutdown.close() }
}
