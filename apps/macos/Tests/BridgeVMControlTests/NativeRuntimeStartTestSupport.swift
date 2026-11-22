import Foundation
@testable import BridgeVMControl

enum NativeRuntimeStartTestSupport {
    static let app = NativeRuntimeControlTestSupport.app
    static let library = NativeRuntimeControlTestSupport.library
    static let target = NativeRuntimeProcessObservation(token: NativeRuntimeControlTestSupport.token, processID: 123)
    static var configuration: HvfEngineConfig {
        .init(targetDiskPath: "/nonexistent/start-fixture.raw", uefiVarsPath: "/nonexistent/start-fixture.fd",
            evidenceDir: "/nonexistent/start-evidence", watchdogMs: nil, ramMiB: 1024, smpCpus: 1,
            clipboardSync: false, shareHostDir: nil, shareGuestDir: nil, virtioNet: false, audioEnabled: false,
            virtioGpu3d: false, nvmeBufferedIO: false, ctlFilePath: "/nonexistent/start-control")
    }
    static func request(_ operation: NativeRuntimeStartRequest.Operation = .start,
                        id: String = UUID().uuidString, vmID: String = "개발-vm", app: String = app) -> NativeRuntimeStartRequest {
        .init(schema: NativeRuntimeStartCodec.requestSchema, operation: operation, requestID: UUID().uuidString,
            library: library, vmID: vmID, appInstanceID: app, operationID: id)
    }
    static func observation(_ request: NativeRuntimeStartRequest,
                            phase: NativeRuntimeStartObservation.Phase = .preparing,
                            target: NativeRuntimeProcessObservation? = nil, proof: NativeRuntimeStartProof? = nil,
                            failure: String? = nil) -> NativeRuntimeStartObservation {
        .init(operationID: request.operationID, phase: phase, target: target, proof: proof, failure: failure)
    }
    static func proof(target: NativeRuntimeProcessObservation = target) -> NativeRuntimeStartProof {
        .init(target: target, manifestSHA256: String(repeating: "a", count: 64), helperPID: 456)
    }
    static func response(_ request: NativeRuntimeStartRequest, observation: NativeRuntimeStartObservation? = nil,
                         refusal: NativeRuntimeStartRefusal? = nil, reason: String? = nil,
                         disposition: NativeRuntimeStartResponse.Disposition? = nil) -> NativeRuntimeStartResponse {
        .init(schema: NativeRuntimeStartCodec.responseSchema, scope: NativeRuntimeStartCodec.scope,
            requestID: request.requestID, library: request.library, vmID: request.vmID, appInstanceID: app,
            requestedOperationID: request.operationID,
            disposition: disposition ?? (refusal != nil ? .refused : request.operation == .start ? .accepted : .status),
            observation: refusal == nil ? observation ?? Self.observation(request) : nil, refusal: refusal, reason: reason)
    }
}
