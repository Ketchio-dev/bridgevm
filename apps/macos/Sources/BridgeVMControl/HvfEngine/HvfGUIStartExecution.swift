import Foundation

enum HvfGUIStartResult: @unchecked Sendable {
    case observedAttachment
    case owned(HvfOwnedRuntimeSpawn)
    case legacy(HvfRuntimeLegacyLaunch.Started)
    case refused(String)
    case failed(HvfRuntimeStartFailureStage, String)
}

struct HvfGUIStartWorkerOutput: @unchecked Sendable {
    let result: HvfGUIStartResult
    let diagnostics: [String]
    init(result: HvfGUIStartResult, diagnostics: [String] = []) {
        self.result = result; self.diagnostics = diagnostics
    }
}

@MainActor
final class HvfGUIStartExecution {
    // Production dependencies serialize their own state. Injected probe, key and
    // launch providers must likewise be safe on the detached worker, not actor-bound.
    struct Input: @unchecked Sendable {
        let config: HvfEngineConfig
        let repoRoot: URL
        let policy: HvfRuntimeStartPolicy
        let keyProvider: VTPMStateKeyProviding
        let processIsRunning: (String) -> Bool
        let launch: (Process) throws -> Void
        let effectAdmission: HvfRuntimeEffectAdmission
        let permit: @MainActor () -> Bool
    }
    typealias Worker = (Input) async -> HvfGUIStartWorkerOutput
    private let session: HvfEngineSession
    private let ticket: HvfGUIStartOperation
    private let policy: HvfRuntimeStartPolicy
    private var task: Task<Void, Never>?

    init(session: HvfEngineSession, ticket: HvfGUIStartOperation, policy: HvfRuntimeStartPolicy) {
        self.session = session; self.ticket = ticket; self.policy = policy
    }

    func begin() {
        let session = session, ticket = ticket
        let input = Input(config: ticket.configuration, repoRoot: session.repoRoot, policy: policy,
            keyProvider: session.vtpmKeyProvider, processIsRunning: session.processIsRunning,
            launch: session.processLaunch, effectAdmission: ticket.effectAdmission,
            permit: { session.admitGUIStartEffect(ticket) })
        let worker = session.guiStartWorker
        task = Task {
            let result = await Task.detached { await worker(input) }.value
            session.finishGUIStartWorker(ticket, result: result)
        }
    }
}
