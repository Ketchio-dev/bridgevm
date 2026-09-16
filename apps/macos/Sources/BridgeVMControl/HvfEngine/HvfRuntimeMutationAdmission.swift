import Foundation

enum HvfRuntimeMutationKind { case vtpmRecoveryExport, vtpmRecoveryRestore, vtpmReset, snapshot }
final class HvfRuntimeEffectAdmission: @unchecked Sendable {
    private let lock = NSLock()
    private var valid = true
    var isValid: Bool { lock.lock(); defer { lock.unlock() }; return valid }
    func invalidate() { lock.lock(); valid = false; lock.unlock() }
}
final class HvfRuntimeMutationReservation: @unchecked Sendable {
    let id = UUID()
    let kind: HvfRuntimeMutationKind
    let configuration: HvfEngineConfig
    private let effectAdmission = HvfRuntimeEffectAdmission()
    init(kind: HvfRuntimeMutationKind, configuration: HvfEngineConfig) {
        self.kind = kind; self.configuration = configuration
    }
    var isActive: Bool { effectAdmission.isValid }
    func invalidate() { effectAdmission.invalidate() }
    func checkEffect() throws {
        guard isActive else { throw NSError(domain: "BridgeVM.RuntimeMutation", code: 1,
            userInfo: [NSLocalizedDescriptionKey: "VM operation reservation is no longer valid"]) }
    }
}

extension HvfEngineSession {
    func runtimeConfigurationChanged() {
        if let reservation = mutationReservation, config != reservation.configuration { reservation.invalidate() }
        if let ticket = ownedStartOperation, config != ticket.configuration { ticket.fail(.configurationChanged) }
    }
    func matchesRuntimeReservation(_ id: UUID) -> Bool {
        if let mutationReservation, mutationReservation.id == id { return mutationReservation.isActive }
        return ownedStartOperation?.operationID == id && ownedStartOperation?.reservesSession == true
    }
    func checkReservedWork(_ id: UUID) -> Bool {
        guard matchesRuntimeReservation(id), !workAdmissionGate.isChecking else { return false }
        let check: LibraryWorkAdmission? = reservedWorkAdmission.map { admission in
            { report in admission(id, report) }
        } ?? workAdmission
        return workAdmissionGate.check(check, reportRefusal: false) == nil
    }
    func reserveRuntimeMutation(kind: HvfRuntimeMutationKind,
                                configuration: HvfEngineConfig? = nil) -> HvfRuntimeMutationReservation? {
        guard !hasActiveRuntimeWork, !workAdmissionGate.isChecking,
              configuration == nil || configuration == config else { return nil }
        let reservation = HvfRuntimeMutationReservation(kind: kind, configuration: config)
        mutationReservation = reservation
        guard checkReservedWork(reservation.id), config == reservation.configuration else {
            reservation.invalidate(); mutationReservation = nil; return nil
        }
        objectWillChange.send()
        return reservation
    }
    func validateRuntimeMutation(_ reservation: HvfRuntimeMutationReservation) -> Bool {
        guard mutationReservation === reservation, reservation.isActive,
              config == reservation.configuration, !runtimeTransitionInProgress,
              !mayHaveOwnedWork, !hasPendingOwnedStart, connectionState == .stopped else { return false }
        return checkReservedWork(reservation.id)
    }
    func finishRuntimeMutation(_ reservation: HvfRuntimeMutationReservation) {
        guard mutationReservation === reservation else { return }
        reservation.invalidate(); mutationReservation = nil
        objectWillChange.send()
    }
}
