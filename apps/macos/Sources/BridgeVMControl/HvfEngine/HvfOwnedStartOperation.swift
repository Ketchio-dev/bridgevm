import Foundation

enum HvfOwnedStartPhase: String, Codable, Sendable { case preparing, awaitingStartup, started, failed, unconfirmed }
enum HvfOwnedStartFailure: String, Error, Codable, Sendable {
    case readiness, preparation, keyUnavailable, configurationChanged, helperUnavailable
    case processLaunch, protocolInvalid, runnerExited, deadlineExceeded
}
enum HvfOwnedStartRefusal: String, Codable, Sendable {
    case busy, runtimeActive, mutationActive, configurationMismatch, operationConflict, admissionRefused
}
struct HvfOwnedStartProof: Equatable, Sendable {
    let target: HvfOwnedRuntimeIdentity
    let manifestSHA256: String
    let helperPID: Int32
    let helperGeneration: UInt64
}
struct HvfOwnedStartObservation: Equatable, Sendable {
    let operationID: UUID
    let expectedSavedConfigurationDigest: String
    let phase: HvfOwnedStartPhase
    let acceptedUptime: TimeInterval
    let deadlineUptime: TimeInterval
    let target: HvfOwnedRuntimeIdentity?
    let proof: HvfOwnedStartProof?
    let failure: HvfOwnedStartFailure?
    let workerPending: Bool
    let mayHaveOwnedWork: Bool
}
@MainActor
final class HvfOwnedStartOperation {
    static let admissionLimit: TimeInterval = 30
    let operationID: UUID
    let configuration: HvfEngineConfig
    let expectedSavedConfigurationDigest: String
    let acceptedUptime: TimeInterval
    let deadlineUptime: TimeInterval
    private(set) var failure: HvfOwnedStartFailure?
    private(set) var proof: HvfOwnedStartProof?
    private(set) var target: HvfOwnedRuntimeIdentity?
    private(set) var workerPending = true
    private(set) var mayHaveOwnedWork = false
    let effectAdmission = HvfRuntimeEffectAdmission()
    private let clock: () -> TimeInterval
    private var deadlineTask: Task<Void, Never>?
    var observation: HvfOwnedStartObservation {
        checkDeadline(now: clock())
        let phase: HvfOwnedStartPhase
        if proof != nil { phase = .started }
        else if failure != nil { phase = workerPending || mayHaveOwnedWork ? .unconfirmed : .failed }
        else { phase = target == nil ? .preparing : .awaitingStartup }
        return .init(operationID: operationID, expectedSavedConfigurationDigest: expectedSavedConfigurationDigest,
            phase: phase, acceptedUptime: acceptedUptime, deadlineUptime: deadlineUptime,
            target: target, proof: proof, failure: failure, workerPending: workerPending, mayHaveOwnedWork: mayHaveOwnedWork)
    }
    var reservesSession: Bool { workerPending || mayHaveOwnedWork || observation.phase == .awaitingStartup }

    init(operationID: UUID, configuration: HvfEngineConfig, expectedSavedConfigurationDigest: String,
         now: TimeInterval, clock: (() -> TimeInterval)? = nil) {
        self.operationID = operationID; self.configuration = configuration
        self.expectedSavedConfigurationDigest = expectedSavedConfigurationDigest
        acceptedUptime = now; deadlineUptime = now + Self.admissionLimit
        self.clock = clock ?? { now }
    }
    func fail(_ reason: HvfOwnedStartFailure) {
        if proof == nil, failure == nil { failure = reason; effectAdmission.invalidate() }
    }
    func checkDeadline(now: TimeInterval) { if proof == nil, now >= deadlineUptime { fail(.deadlineExceeded) } }
    func finishWorker(target: HvfOwnedRuntimeIdentity?) {
        workerPending = false; self.target = target; mayHaveOwnedWork = target != nil
    }
    func watchDeadline(_ expired: @escaping @MainActor () -> Void) {
        let delay = max(0, deadlineUptime - clock())
        deadlineTask = Task { [weak self] in
            do { try await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000)) } catch { return }
            guard let self else { return }
            self.checkDeadline(now: self.clock()); expired()
        }
    }
    deinit { deadlineTask?.cancel() }
    func observe(_ controller: HvfOwnedRunController, now: TimeInterval) {
        guard target == controller.identity else { return }
        mayHaveOwnedWork = controller.mayHaveOwnedWork
        guard proof == nil else { return }
        checkDeadline(now: now)
        guard failure == nil else { return }
        if controller.failure != nil { fail(.protocolInvalid); return }
        if controller.runnerExit != nil || controller.ledger.complete != nil { fail(.runnerExited); return }
        if controller.ledger.ready, let child = controller.ledger.initialHelper {
            proof = .init(target: controller.identity, manifestSHA256: controller.ledger.manifestSHA256,
                helperPID: child.pid, helperGeneration: 0)
        }
    }
}
@MainActor
enum HvfOwnedStartAdmission {
    case accepted(HvfOwnedStartOperation), existing(HvfOwnedStartOperation), refused(HvfOwnedStartRefusal)
}
