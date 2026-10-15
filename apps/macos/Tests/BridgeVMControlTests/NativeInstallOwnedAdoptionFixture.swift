import Combine
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOwnedAdoptionFixture {
    let store = HvfWindowsInstallStoreFixture()
    let queue = HvfWindowsInstallPipelineQueue()
    let validator = HvfWindowsInstallValidationProbe()
    let inspection = HvfWindowsInstallValidationProbe()
    let process = NativeInstallOwnedAdoptionProcess()
    var session: HvfWindowsInstallSession?
    var onValidating: ((HvfWindowsInstallSession) -> Void)?
    var subscription: AnyCancellable?
    var made = 0, sourceChecks = 0, mediaPreparations = 0, finalizations = 0

    func library(refuseStart: Bool = false) -> LibraryModel {
        LibraryModel(rootURL: store.root, migrateLegacy: false, installSessionFactory: { [unowned self] plan in
            made += 1
            let execution = HvfWindowsInstallExecution(process: process,
                verifySource: { [unowned self] _ in sourceChecks += 1; return false },
                prepareMedia: { [unowned self] _ in mediaPreparations += 1 },
                finalize: { [unowned self] _ in finalizations += 1 })
            let value = HvfWindowsInstallSession(plan: plan,
                validate: { [validator] in validator.validate($0) }, schedule: queue.enqueue,
                execution: execution, recovery: .init(inspect: { [inspection] plan in
                    _ = inspection.validate(plan); return .fresh
                }, recover: { _, _ in throw CocoaError(.featureUnsupported) }))
            if refuseStart { value.finish(.done) }
            session = value
            subscription = value.$stage.sink { [weak self, weak value] stage in
                guard stage == .validating, let value else { return }
                self?.onValidating?(value)
            }
            return value
        })
    }

    func request(_ operation: NativeInstallControlRequest.Operation,
                 operationID: UUID? = nil) throws -> NativeInstallControlRequest {
        let config = store.config()
        let library = try NativeRuntimeLibraryHandle.open(rootURL: store.root, create: false)
        return .init(schema: NativeInstallControlCodec.requestSchema, operation: operation,
            requestID: UUID().uuidString, library: library.identity, vmID: config.slug,
            appInstanceID: UUID().uuidString,
            expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: config),
            operationID: operationID?.uuidString)
    }

    func waitForAcknowledgment() async {
        for _ in 0..<200 {
            if session?.stage == .cancelled || session?.stage == .done || !queue.jobs.isEmpty { return }
            try? await Task.sleep(nanoseconds: 1_000_000)
        }
        XCTFail("The synthetic start acknowledgment did not settle")
    }

    func clean() {
        subscription = nil; onValidating = nil
        queue.discard() // Never execute an install pipeline, even on the failing baseline.
        store.clean()
    }
}

@MainActor
final class NativeInstallOwnedAdoptionProcess: HvfWindowsInstallProcessRunning {
    var resets = 0, cancellations = 0, requests = 0
    func reset() { resets += 1 }
    func cancel() { cancellations += 1 }
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
             progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        requests += 1
        XCTFail("An owned-install admission test must never dispatch a process")
        return false
    }
}
