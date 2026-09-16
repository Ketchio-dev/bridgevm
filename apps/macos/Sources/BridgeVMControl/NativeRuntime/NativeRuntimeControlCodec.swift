import Foundation

enum NativeRuntimeControlCodec {
    static let requestSchema = "bridgevm.app-runtime-control-request.v2"
    static let responseSchema = "bridgevm.app-runtime-control.v2"
    static let scope = "app-owned-runtime-control"

    static func validate(_ request: NativeRuntimeControlRequest) throws {
        let status = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
            requestID: request.requestID, library: request.library, vmID: request.vmID,
            savedConfiguration: .init(state: .missing, digest: nil))
        try NativeRuntimeCodec.validate(status)
        guard request.schema == requestSchema, NativeRuntimeCodec.validUUID(request.operationID),
              NativeRuntimeCodec.validUUID(request.target.appInstanceID),
              NativeRuntimeCodec.validUUID(request.target.runToken), request.target.processID > 0,
              NativeRuntimeCodec.validDigest(request.target.acceptedConfigurationDigest)
        else { throw NativeRuntimeError.invalidMessage }
    }

    static func validate(_ response: NativeRuntimeControlResponse, for request: NativeRuntimeControlRequest) throws {
        try validate(request)
        guard response.schema == responseSchema, response.scope == scope,
              response.requestID == request.requestID, response.library == request.library,
              response.vmID == request.vmID, response.target == request.target,
              response.requestedOperationID == request.operationID,
              NativeRuntimeCodec.validUUID(response.appInstanceID) else { throw NativeRuntimeError.invalidMessage }
        if response.disposition == .refused {
            guard response.refusal != nil, response.observation == nil,
                  response.appInstanceID == request.target.appInstanceID || response.refusal == .ownerChanged
            else { throw NativeRuntimeError.invalidMessage }
            return
        }
        guard response.refusal == nil, response.appInstanceID == request.target.appInstanceID,
              let observation = response.observation,
              request.operation == .stop || response.disposition == .status,
              response.disposition != .status || request.operation == .stopStatus,
              response.disposition == .existing || observation.operationID == request.operationID
        else { throw NativeRuntimeError.invalidMessage }
        try validate(observation, target: request.target)
    }
}
