import Foundation
@testable import BridgeVMControl

enum NativeRuntimeControlTestSupport {
    static let app = "A0000000-0000-0000-0000-000000000001"
    static let token = "B0000000-0000-0000-0000-000000000001"
    static let library = NativeRuntimeLibraryIdentity(canonicalPath: "/private/tmp/control-fixture", device: 1, inode: 2, uid: 501)
    static let target = NativeRuntimeStopTarget(appInstanceID: app, runToken: token, processID: 123,
                                               acceptedConfigurationDigest: String(repeating: "a", count: 64))
    static func request(_ operation: NativeRuntimeControlRequest.Operation = .stop,
                        id: String = UUID().uuidString, target: NativeRuntimeStopTarget = target,
                        vmID: String = "개발-vm") -> NativeRuntimeControlRequest {
        .init(schema: NativeRuntimeControlCodec.requestSchema, operation: operation,
              requestID: UUID().uuidString, library: library, vmID: vmID, operationID: id, target: target)
    }
    static func observation(_ request: NativeRuntimeControlRequest,
                            phase: NativeRuntimeStopObservation.Phase = .guestGrace,
                            id: String? = nil, proof: Bool = false, exit: Bool = false) -> NativeRuntimeStopObservation {
        .init(operationID: id ?? request.operationID, phase: phase, acceptedUptime: 10, deadlineUptime: 202,
            supervisorComplete: proof ? .init(operationID: nil, helperSpawnedCount: 1, helperReapedCount: 1,
                swtpmSpawnedCount: 0, swtpmReapedCount: 0, mediaLeaseDisposition: "releasedAfterReap",
                runtimeDirectoryDisposition: "notCreated", failureCode: nil) : nil,
            runnerExit: exit ? .init(process: .init(token: request.target.runToken, processID: request.target.processID),
                                    reason: "exit", status: 1) : nil,
            failure: phase == .unconfirmed ? .cleanupUnconfirmed : nil)
    }
    static func response(_ request: NativeRuntimeControlRequest,
                         observation: NativeRuntimeStopObservation? = nil,
                         refusal: NativeRuntimeStopRefusal? = nil,
                         disposition: NativeRuntimeControlResponse.Disposition? = nil) -> NativeRuntimeControlResponse {
        .init(schema: NativeRuntimeControlCodec.responseSchema, scope: NativeRuntimeControlCodec.scope,
              requestID: request.requestID, library: request.library, vmID: request.vmID, appInstanceID: app,
              requestedOperationID: request.operationID, target: request.target,
              disposition: disposition ?? (refusal == nil ? (request.operation == .stop ? .accepted : .status) : .refused),
              observation: refusal == nil ? observation ?? Self.observation(request) : nil, refusal: refusal)
    }
}
