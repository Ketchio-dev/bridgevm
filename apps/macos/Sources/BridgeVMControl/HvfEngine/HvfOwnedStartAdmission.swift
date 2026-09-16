import Foundation

extension HvfEngineSession {
    func requestOwnedStart(configuration: HvfEngineConfig, operationID: UUID,
                           expectedSavedConfigurationDigest: String,
                           onReserved: @MainActor (HvfOwnedStartOperation) throws -> Void,
                           revalidate: @escaping @MainActor () -> Bool) -> HvfOwnedStartAdmission {
        if let previous = ownedStartOperation, previous.operationID == operationID {
            guard previous.configuration == configuration,
                  previous.expectedSavedConfigurationDigest == expectedSavedConfigurationDigest else { return .refused(.operationConflict) }
            do { try onReserved(previous) } catch { return .refused(.admissionRefused) }
            return .existing(previous)
        }
        guard !runtimeTransitionInProgress, !workAdmissionGate.isChecking else { return .refused(.busy) }
        guard mutationReservation == nil else { return .refused(.mutationActive) }
        guard !hasPendingOwnedStart, !mayHaveOwnedWork, process?.isRunning != true,
              !hasRetainedAttachment, connectionState == .stopped else { return .refused(.runtimeActive) }
        guard config == configuration, expectedSavedConfigurationDigest.utf8.count == 64,
              expectedSavedConfigurationDigest.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else {
            return .refused(.configurationMismatch)
        }
        let ticket = HvfOwnedStartOperation(operationID: operationID, configuration: configuration,
            expectedSavedConfigurationDigest: expectedSavedConfigurationDigest, now: ownedStartNow(), clock: ownedStartNow)
        ownedStartOperation = ticket
        do { try onReserved(ticket) }
        catch { releaseUnstartedTicket(ticket); return .refused(.admissionRefused) }
        guard checkReservedWork(operationID), revalidate(), config == configuration else {
            releaseUnstartedTicket(ticket); return .refused(.admissionRefused)
        }
        let execution = HvfOwnedStartExecution(session: self, ticket: ticket, revalidate: revalidate)
        ownedStartExecution = execution
        objectWillChange.send()
        execution.begin()
        return .accepted(ticket)
    }
    private func releaseUnstartedTicket(_ ticket: HvfOwnedStartOperation) {
        ticket.fail(.configurationChanged); ticket.finishWorker(target: nil)
        if ownedStartOperation === ticket { ownedStartOperation = nil }
    }
    func ownedStartObservation(operationID: UUID) -> HvfOwnedStartObservation? {
        guard ownedStartOperation?.operationID == operationID else { return nil }
        refreshOwnedStart()
        return ownedStartOperation?.observation
    }
    func refreshOwnedStart() {
        guard let ticket = ownedStartOperation else { return }
        ticket.checkDeadline(now: ownedStartNow())
        if let controller = ownedController, ticket.target == controller.identity {
            ticket.observe(controller, now: ownedStartNow())
            if ticket.failure != nil, controller.mayHaveOwnedWork, controller.operation == nil {
                _ = controller.requestStop(target: controller.identity, operationID: UUID(), allowGuestGrace: false)
            }
        }
    }
    func admitOwnedStartEffect(_ ticket: HvfOwnedStartOperation, revalidate: @MainActor () -> Bool) -> Bool {
        guard ownedStartOperation === ticket, ticket.workerPending else { return false }
        ticket.checkDeadline(now: ownedStartNow())
        guard ticket.failure == nil else { return false }
        guard config == ticket.configuration, checkReservedWork(ticket.operationID), revalidate() else {
            ticket.fail(.configurationChanged); return false
        }
        ticket.checkDeadline(now: ownedStartNow())
        return ticket.failure == nil
    }
    func finishOwnedStartWorker(_ ticket: HvfOwnedStartOperation, result: Result<HvfOwnedRuntimeSpawn, HvfOwnedStartFailure>,
                                revalidate: @MainActor () -> Bool) {
        guard ownedStartOperation === ticket else { return }
        ticket.checkDeadline(now: ownedStartNow())
        switch result {
        case let .failure(reason): ticket.fail(reason); ticket.finishWorker(target: nil)
        case let .success(spawn):
            _ = admitOwnedStartEffect(ticket, revalidate: revalidate)
            let launched = HvfOwnedRuntimeLaunch.adopt(spawn)
            let controller = launched.controller
            ownedController = controller; process = controller.process; ownedProcessIdentity = controller.identity
            ticket.finishWorker(target: controller.identity)
            lastOwnedExit = nil; attachedToExistingProcess = false
            let token = controller.identity.token
            controller.onChange = { [weak self] in self?.ownedRuntimeChanged(token: token) }
            controller.onDiagnostic = { [weak self] message in
                guard self?.ownedController?.identity.token == token else { return }
                self?.append(.unknown(message))
            }
            closeLiveInput(); resetObservedRuntimeState(clearEvents: true)
            connectionState = ticket.failure == nil ? .booting : .stopping
            beginOwnedInputBoot(); controller.activate(helloFrame: launched.helloFrame); startPolling()
        }
        ownedStartExecution = nil
        refreshOwnedStart()
        objectWillChange.send()
    }
}
