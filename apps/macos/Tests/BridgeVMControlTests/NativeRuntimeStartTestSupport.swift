import Foundation
@testable import BridgeVMControl

enum NativeRuntimeStartTestSupport {
    static let app = NativeRuntimeControlTestSupport.app
    static let library = NativeRuntimeControlTestSupport.library
    static let digest = String(repeating: "a", count: 64)
    static let target = NativeRuntimeProcessObservation(token: NativeRuntimeControlTestSupport.token, processID: 123)
    static var configuration: HvfEngineConfig {
        .init(targetDiskPath: "/nonexistent/start-fixture.raw", uefiVarsPath: "/nonexistent/start-fixture.fd",
            evidenceDir: "/nonexistent/start-evidence", watchdogMs: nil, ramMiB: 1024, smpCpus: 1,
            clipboardSync: false, shareHostDir: nil, shareGuestDir: nil, virtioNet: false, audioEnabled: false,
            virtioGpu3d: false, nvmeBufferedIO: false, ctlFilePath: "/nonexistent/start-control")
    }
    static func request(_ operation: NativeRuntimeStartRequest.Operation = .start,
                        id: String = UUID().uuidString, vmID: String = "개발-vm", app: String = app,
                        digest: String = digest) -> NativeRuntimeStartRequest {
        .init(schema: NativeRuntimeStartCodec.requestSchema, operation: operation, requestID: UUID().uuidString,
            library: library, vmID: vmID, appInstanceID: app, expectedSavedConfigurationDigest: digest, operationID: id)
    }
    @MainActor static func ticket(_ request: NativeRuntimeStartRequest) -> HvfOwnedStartOperation {
        .init(operationID: UUID(uuidString: request.operationID)!, configuration: configuration,
              expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest, now: 10)
    }
    static func observation(_ request: NativeRuntimeStartRequest,
                            phase: NativeRuntimeStartObservation.Phase = .preparing,
                            target: NativeRuntimeProcessObservation? = nil, proof: NativeRuntimeStartProof? = nil,
                            failure: NativeRuntimeStartFailure? = nil, worker: Bool = true, owned: Bool = false,
                            deadline: Double = 40) -> NativeRuntimeStartObservation {
        .init(operationID: request.operationID, expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest,
            phase: phase, acceptedUptime: 10, deadlineUptime: deadline, target: target, proof: proof,
            failure: failure, workerPending: worker, mayHaveOwnedWork: owned)
    }
    static func proof(target: NativeRuntimeProcessObservation = target, digest: String = digest,
                      helper: Int32 = 456, generation: UInt64 = 0) -> NativeRuntimeStartProof {
        .init(target: target, manifestSHA256: digest, helperPID: helper, helperGeneration: generation)
    }
    static func response(_ request: NativeRuntimeStartRequest, observation: NativeRuntimeStartObservation? = nil,
                         refusal: NativeRuntimeStartRefusal? = nil,
                         disposition: NativeRuntimeStartResponse.Disposition? = nil) -> NativeRuntimeStartResponse {
        .init(schema: NativeRuntimeStartCodec.responseSchema, scope: NativeRuntimeStartCodec.scope,
            requestID: request.requestID, library: request.library, vmID: request.vmID, appInstanceID: app,
            expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest, requestedOperationID: request.operationID,
            disposition: disposition ?? (refusal != nil ? .refused : request.operation == .start ? .accepted : .status),
            observation: refusal == nil ? observation ?? Self.observation(request) : nil, refusal: refusal)
    }
}
