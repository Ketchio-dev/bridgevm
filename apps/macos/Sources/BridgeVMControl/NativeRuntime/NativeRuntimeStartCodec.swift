import Foundation

enum NativeRuntimeStartCodec {
    static let requestSchema = "bridgevm.app-runtime-start-request.v1"
    static let responseSchema = "bridgevm.app-runtime-start.v1"
    static let scope = "app-owned-runtime-start"

    static func validate(_ request: NativeRuntimeStartRequest) throws {
        try NativeRuntimeCodec.validate(NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema,
            operation: "status", requestID: request.requestID, library: request.library, vmID: request.vmID,
            savedConfiguration: .init(state: .present, digest: request.expectedSavedConfigurationDigest)))
        guard request.schema == requestSchema, NativeRuntimeCodec.validUUID(request.operationID),
              NativeRuntimeCodec.validUUID(request.appInstanceID) else { throw NativeRuntimeError.invalidMessage }
    }

    static func validate(_ response: NativeRuntimeStartResponse, for request: NativeRuntimeStartRequest) throws {
        try validate(request)
        guard response.schema == responseSchema, response.scope == scope,
              response.requestID == request.requestID, response.library == request.library,
              response.vmID == request.vmID, response.requestedOperationID == request.operationID,
              response.expectedSavedConfigurationDigest == request.expectedSavedConfigurationDigest,
              NativeRuntimeCodec.validUUID(response.appInstanceID) else { throw NativeRuntimeError.invalidMessage }
        if response.disposition == .refused {
            guard response.refusal != nil, response.observation == nil,
                  response.appInstanceID == request.appInstanceID || response.refusal == .ownerChanged
            else { throw NativeRuntimeError.invalidMessage }
            return
        }
        guard response.refusal == nil, response.appInstanceID == request.appInstanceID,
              let observation = response.observation,
              observation.operationID == request.operationID,
              observation.expectedSavedConfigurationDigest == request.expectedSavedConfigurationDigest,
              request.operation == .start || response.disposition == .status,
              response.disposition != .status || request.operation == .startStatus
        else { throw NativeRuntimeError.invalidMessage }
        try validate(observation)
    }
}
