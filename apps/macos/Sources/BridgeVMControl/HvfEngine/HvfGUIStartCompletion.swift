import Foundation

extension HvfEngineSession {
    func finishGUIStartWorker(_ ticket: HvfGUIStartOperation, result: HvfGUIStartWorkerOutput) {
        guard guiStartOperation === ticket else { return }
        let admitted = admitGUIStartEffect(ticket)
        let invalidation = ticket.invalidationReason ?? "VM 시작 예약이 변경되었습니다."
        let outcome: HvfRuntimeStartOutcome
        switch result.result {
        case .observedAttachment:
            if admitted, observeGUIAttachment(ticket) { outcome = .observedAttachment }
            else { outcome = .refused(ticket.invalidationReason ?? invalidation) }
        case let .owned(spawn):
            // Even a stale result owns a real process. Adopt it before publishing any failure.
            guiStartCleanupOnly = !admitted
            let launched = HvfOwnedRuntimeLaunch.adopt(spawn)
            let controller = launched.controller
            ownedController = controller; process = controller.process
            ownedProcessIdentity = controller.identity
            lastOwnedExit = nil; attachedToExistingProcess = false
            let token = controller.identity.token
            controller.onChange = { [weak self] in self?.ownedRuntimeChanged(token: token) }
            controller.onDiagnostic = { [weak self] message in
                guard self?.ownedController?.identity.token == token else { return }
                self?.append(.unknown(message))
            }
            prepareGUIRuntimeObservation()
            controller.activate(helloFrame: launched.helloFrame)
            if beginGUIRuntimeObservation(ticket) {
                outcome = .ownedLaunchAccepted(controller.identity)
            } else {
                guiStartCleanupOnly = true
                cancelOrderedInputTarget(); closeLiveInput()
                connectionState = .stopping
                _ = controller.requestStop(target: controller.identity, operationID: UUID(), allowGuestGrace: false)
                startInvalidatedOwnedCleanup(controller)
                outcome = .refused(ticket.invalidationReason ?? invalidation)
            }
        case let .legacy(started):
            guiStartCleanupOnly = !admitted || started.keyDeliveryFailure != nil
            ownedController = nil; process = started.process
            let identity = HvfOwnedRuntimeIdentity(token: UUID(), processID: started.process.processIdentifier)
            ownedProcessIdentity = identity; lastOwnedExit = nil; attachedToExistingProcess = false
            prepareGUIRuntimeObservation()
            if started.keyDeliveryFailure == nil, beginGUIRuntimeObservation(ticket) {
                outcome = .ownedLaunchAccepted(identity)
            } else {
                let stillAdmitted = admitGUIStartEffect(ticket)
                guiStartCleanupOnly = true
                connectionState = .stopping
                startInvalidatedLegacyCleanup(started.process)
                if stillAdmitted, let failure = started.keyDeliveryFailure {
                    outcome = .failed(.keyDelivery, failure)
                } else { outcome = .refused(ticket.invalidationReason ?? invalidation) }
            }
        case let .refused(reason): outcome = .refused(admitted ? reason : invalidation)
        case let .failed(stage, detail): outcome = admitted ? .failed(stage, detail) : .refused(invalidation)
        }
        ticket.finish(outcome)
        guiStartExecution = nil
        for diagnostic in result.diagnostics { append(.unknown(diagnostic)) }
        if let failure = guiStartFailureText { append(.unknown(failure)) }
        objectWillChange.send()
    }

    private func prepareGUIRuntimeObservation() {
        timer?.invalidate(); timer = nil
        closeLiveInput(); resetObservedRuntimeState(clearEvents: true)
    }

    private func observeGUIAttachment(_ ticket: HvfGUIStartOperation) -> Bool {
        prepareGUIRuntimeObservation()
        guard admitGUIStartEffect(ticket) else { return false }
        process = nil; ownedController = nil
        attachedToExistingProcess = true
        nextAttachedLivenessCheck = HvfAttachedLivenessSchedule.next(after: Date())
        connectionState = .booting
        if admitGUIStartEffect(ticket) {
            startPolling()
            if admitGUIStartEffect(ticket) {
                append(.unknown("attached to the already running HVF engine; duplicate launch prevented"))
                if admitGUIStartEffect(ticket) { return true }
            }
        }
        // An external runtime was only observed. Invalidating observation never stops it.
        markStopped()
        return false
    }

    private func beginGUIRuntimeObservation(_ ticket: HvfGUIStartOperation) -> Bool {
        guard admitGUIStartEffect(ticket) else { return false }
        connectionState = .booting
        guard admitGUIStartEffect(ticket) else { return false }
        beginOwnedInputBoot(); startPolling()
        return admitGUIStartEffect(ticket)
    }

    func pollGUIStartCleanup() {
        if let controller = ownedController { controller.tick() }
        else if let process, !process.isRunning { markStopped() }
    }

    private func startInvalidatedOwnedCleanup(_ controller: HvfOwnedRunController) {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard self?.ownedController === controller else { return }
                controller.tick()
            }
        }
        controller.tick()
    }

    private func startInvalidatedLegacyCleanup(_ child: Process) {
        timer?.invalidate()
        cancelOrderedInputTarget(); closeLiveInput()
        // Observe only this owned child; never read or send commands using a changed configuration.
        if child.isRunning { child.terminate() }
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self, self.process === child else { return }
                if !child.isRunning { self.markStopped() }
            }
        }
        if !child.isRunning { markStopped() }
    }
}
