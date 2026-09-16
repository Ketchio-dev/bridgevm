import Foundation

enum HvfOwnedStopPhase: String, Codable, Sendable {
    case guestGrace, cancelling, awaitingRunnerExit, completed, unconfirmed
}

enum HvfOwnedStopFailure: String, Codable, Sendable {
    case protocolInvalid, channelFailed, cleanupUnconfirmed, deadlineExceeded
    case runnerExitedWithoutComplete, cleanupFailed
}

enum HvfOwnedStopRefusal: String, Codable, Sendable {
    case notOwned, supervisionUnavailable, targetMismatch, operationConflict
}

struct HvfOwnedStopObservation: Equatable, Sendable {
    let operationID: UUID
    let target: HvfOwnedRuntimeIdentity
    let phase: HvfOwnedStopPhase
    let acceptedUptime: TimeInterval
    let deadlineUptime: TimeInterval
    let supervisorComplete: HvfOwnedRuntimeComplete?
    let runnerExit: HvfOwnedRuntimeExit?
    let failure: HvfOwnedStopFailure?
    var isComplete: Bool { phase == .completed && failure == nil }
}

@MainActor
final class HvfOwnedStopOperation {
    let operationID: UUID
    let target: HvfOwnedRuntimeIdentity
    private(set) var observation: HvfOwnedStopObservation

    init(operationID: UUID, target: HvfOwnedRuntimeIdentity, now: TimeInterval,
         deadline: TimeInterval, phase: HvfOwnedStopPhase) {
        self.operationID = operationID; self.target = target
        observation = .init(operationID: operationID, target: target, phase: phase,
            acceptedUptime: now, deadlineUptime: deadline,
            supervisorComplete: nil, runnerExit: nil, failure: nil)
    }

    func update(phase: HvfOwnedStopPhase, deadline: TimeInterval? = nil,
                complete: HvfOwnedRuntimeComplete?, exit: HvfOwnedRuntimeExit?,
                failure: HvfOwnedStopFailure?) {
        observation = .init(operationID: operationID, target: target, phase: phase,
            acceptedUptime: observation.acceptedUptime,
            deadlineUptime: deadline ?? observation.deadlineUptime,
            supervisorComplete: complete, runnerExit: exit, failure: failure)
    }
}

@MainActor
enum HvfOwnedStopAdmission {
    case accepted(HvfOwnedStopOperation)
    case existing(HvfOwnedStopOperation)
    case refused(HvfOwnedStopRefusal)
}
