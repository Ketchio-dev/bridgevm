import Foundation

/// What a guest-grace stop does when the grace ends and Windows is still running.
enum HvfOwnedStopGraceEnd: Sendable {
    /// The CLI's bounded stop: request runner cancellation (180 s, then 12 s to observe cleanup).
    case cancelRunner
    /// The app's Stop and quit: keep waiting; only an explicit Force Stop ends the run.
    case offerForceStop
}

@MainActor
final class HvfOwnedRunController {
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
    private var eventObserver: HvfOwnedRuntimeEventObserver?
    private var graceDeadline: TimeInterval?
    private var holdsAfterGrace = false
    private(set) var graceExpired = false
    private var forceDeadline: TimeInterval?
    private var forceRequested = false
    private var cancelled = false
    private var serviceReady = false
    private var guestRequestSent = false
    private var powerButtonPressed = false
    private var guestEffectsPending = 0
    private var outputEnded = false
    private var protocolValid = true
    private var teardownConfirmedAt: TimeInterval?
    var onChange: (() -> Void)?
    var onDiagnostic: ((String) -> Void)?
    var teardownConfirmed: Bool { protocolValid && outputEnded && guestEffectsPending == 0 && ledger.complete != nil && runnerExit != nil }

    init(identity: HvfOwnedRuntimeIdentity, config: HvfEngineConfig, process: Process,
         manifestSHA256: String, channel: HvfOwnedRuntimeChannel,
         guestShutdown: HvfOwnedGuestShutdown,
         now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }) {
        self.identity = identity; self.config = config; self.process = process
        ledger = .init(identity: identity, manifestSHA256: manifestSHA256)
        self.channel = channel; self.guestShutdown = guestShutdown; self.now = now
    }

    func activate(helloFrame: Data) {
        eventObserver = HvfOwnedRuntimeEventObserver(events: channel.events, process: process,
            receive: { [weak self] in self?.receive($0) }, terminated: { [weak self] in self?.tick() })
        channel.start(helloFrame: helloFrame)
    }

    func requestStop(target: HvfOwnedRuntimeIdentity, operationID: UUID, allowGuestGrace: Bool = true,
                     afterGrace: HvfOwnedStopGraceEnd = .cancelRunner,
                     pressPowerButton: (@MainActor () -> Bool)? = nil) -> HvfOwnedStopAdmission {
        guard target == identity else { return .refused(.targetMismatch) }
        if let operation { return .existing(operation) }
        let time = now()
        let useGrace = allowGuestGrace && ledger.ready && ledger.complete == nil && failure == nil && runnerExit == nil
        let phase: HvfOwnedStopPhase = useGrace ? .guestGrace : .cancelling
        let ticket = HvfOwnedStopOperation(operationID: operationID, target: identity, now: time,
            deadline: time + (useGrace ? Self.guestGrace : 0) + Self.completionObservation, phase: phase)
        operation = ticket // Publish admission before invoking any effect or callback.
        holdsAfterGrace = useGrace && afterGrace == .offerForceStop
        do { try ledger.admitStop(operationID: operationID) }
        catch { recordFailure(.protocolInvalid) }
        if ledger.complete != nil { refresh() }
        else if useGrace {
            graceDeadline = time + Self.guestGrace
            requestGuestIfReady()
            if !serviceReady, let pressPowerButton { pressGuestPowerButton(pressPowerButton) }
        } else { cancelRunner() }
        tick()
        return .accepted(ticket)
    }

    /// With the guest agent ready, Windows is told to close its apps without asking and turn off;
    /// the runner is cancelled if it is still running after `forcedGuestWindow`. Without the agent the
    /// runner is cancelled at once (the helper still persists vars and disk on TERM).
    func requestForceStop(_ target: HvfForceStopTarget) -> HvfForceStopResult {
        guard target.identity == identity else { return .refused(.targetChanged) }
        guard let operation, operation.operationID == target.operationID else { return .refused(.operationChanged) }
        guard waitsForGuestShutdown else { return .refused(.notWaiting) }
        graceDeadline = nil; forceRequested = true
        if serviceReady {
            let time = now(), limit = time + Self.forcedGuestWindow
            let deadline = holdsAfterGrace ? limit
                : min(limit, operation.observation.deadlineUptime - Self.completionObservation)
            forceDeadline = deadline
            if holdsAfterGrace { // the held Stop had no deadline; the escalation has one
                operation.update(phase: .cancelling, deadline: deadline + Self.completionObservation,
                                 complete: ledger.complete, exit: runnerExit, failure: failure)
            }
            sendGuest(.forceShutdown)
        } else { cancelRunner() }
        tick()
        return .requested
    }

    /// A Stop is waiting for Windows to shut itself down: Windows still runs and takes input, and the
    /// Stop may still be escalated. A held Stop stays so after its grace; Force Stop or cancellation ends it.
    var waitsForGuestShutdown: Bool {
        guard let operation, operation.observation.phase == .guestGrace, let deadline = graceDeadline,
              holdsAfterGrace || now() < deadline, !cancelled, failure == nil, ledger.complete == nil,
              runnerExit == nil, process.isRunning else { return false }
        return true
    }
    var guestStopRequested: Bool { guestRequestSent || powerButtonPressed }

    /// Set by this boot generation's `SERVICE start`; a guest reset record drops it, so an unsent request waits.
    func guestServiceReady() { serviceReady = true; requestGuestIfReady() }
    func guestServiceLost() { serviceReady = false }

    func tick() {
        if runnerExit == nil, !process.isRunning {
            runnerExit = HvfOwnedRuntimeExit(identity: identity, process: process)
            guestShutdown.close()
        }
        if let deadline = graceDeadline, now() >= deadline, ledger.complete == nil {
            if holdsAfterGrace { graceExpired = true } else { cancelRunner() }
        }
        if let deadline = forceDeadline, now() >= deadline, ledger.complete == nil { cancelRunner() }
        let observedAt = now()
        if teardownConfirmed, teardownConfirmedAt == nil { teardownConfirmedAt = observedAt }
        if let ticket = operation, !awaitsUserDecision,
           (teardownConfirmedAt ?? observedAt) >= ticket.observation.deadlineUptime {
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
                if ledger.complete != nil { graceDeadline = nil; forceDeadline = nil; guestShutdown.close() }
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

    /// A held Stop has no deadline until the user escalates; Windows may be waiting on an app.
    private var awaitsUserDecision: Bool { holdsAfterGrace && !forceRequested && !cancelled }

    /// Without the agent (login screen, early boot) Windows still honours its ACPI power button and shuts
    /// down as for `shutdown.exe /s`. The grace and Force Stop stay as they are, and the agent's request
    /// still follows if the agent becomes ready while Windows is running.
    private func pressGuestPowerButton(_ press: @MainActor () -> Bool) {
        powerButtonPressed = press()
        onDiagnostic?(powerButtonPressed ? "Guest agent not ready; pressed the guest's ACPI power button"
            : "Guest power button unavailable; waiting for the guest agent")
    }

    private func requestGuestIfReady() {
        guard operation != nil, graceDeadline != nil, serviceReady, !guestRequestSent,
              runnerExit == nil, ledger.complete == nil else { return }
        guestRequestSent = true
        sendGuest(.shutdown)
    }

    private func sendGuest(_ command: HvfGuestStopCommand) {
        guestEffectsPending += 1
        let token = identity.token
        guestShutdown.request(command) { [weak self] delivered in
            Task { @MainActor [weak self] in
                guard let self, self.identity.token == token else { return }
                self.guestEffectsPending -= 1
                let waiting = command == .shutdown ? self.graceDeadline != nil : self.forceDeadline != nil
                guard waiting, !self.cancelled, self.runnerExit == nil, self.ledger.complete == nil else {
                    self.tick(); return
                }
                if delivered { self.onDiagnostic?("Guest \(command.line) delivered; waiting for supervised cleanup") }
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
        let limit = now() + Self.completionObservation
        // A held Stop has had no deadline, so its cancellation gets one; a Force Stop already set its own.
        let deadline = holdsAfterGrace && forceDeadline == nil ? limit
            : min(operation.observation.deadlineUptime, limit)
        cancelled = true; graceDeadline = nil; forceDeadline = nil; guestShutdown.close()
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
    deinit { eventObserver?.cancel(); channel.close(); guestShutdown.close() }
}
