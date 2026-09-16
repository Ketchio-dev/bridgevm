import Foundation

enum NativeInstallControlCodec {
    static let requestSchema = "bridgevm.app-install-control-request.v1"
    static let responseSchema = "bridgevm.app-install-control.v1"
    static let scope = "app-owned-install"

    static func validate(_ request: NativeInstallControlRequest) throws {
        try NativeRuntimeCodec.validate(NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema,
            operation: "status", requestID: request.requestID, library: request.library, vmID: request.vmID,
            savedConfiguration: .init(state: .present, digest: request.expectedSavedConfigurationDigest)))
        guard request.schema == requestSchema, NativeRuntimeCodec.validUUID(request.appInstanceID) else {
            throw NativeRuntimeError.invalidMessage
        }
        switch request.operation {
        case .install:
            guard let id = request.operationID, NativeRuntimeCodec.validUUID(id) else {
                throw NativeRuntimeError.invalidMessage
            }
        case .installStatus, .installCancel:
            guard request.operationID == nil else { throw NativeRuntimeError.invalidMessage }
        }
    }

    static func validate(_ response: NativeInstallControlResponse,
                         for request: NativeInstallControlRequest) throws {
        try validate(request)
        guard response.schema == responseSchema, response.scope == scope,
              response.requestID == request.requestID, response.library == request.library,
              response.vmID == request.vmID,
              response.expectedSavedConfigurationDigest == request.expectedSavedConfigurationDigest,
              response.requestedOperationID == request.operationID,
              NativeRuntimeCodec.validUUID(response.appInstanceID) else {
            throw NativeRuntimeError.invalidMessage
        }
        if response.disposition == .refused {
            guard response.refusal != nil, response.observation == nil,
                  response.appInstanceID == request.appInstanceID || response.refusal == .ownerChanged else {
                throw NativeRuntimeError.invalidMessage
            }
            return
        }
        guard response.refusal == nil, response.appInstanceID == request.appInstanceID,
              let observation = response.observation else { throw NativeRuntimeError.invalidMessage }
        switch request.operation {
        case .install:
            guard response.disposition == .accepted || response.disposition == .existing,
                  response.disposition == .existing || observation.operationID == request.operationID else {
                throw NativeRuntimeError.invalidMessage
            }
        case .installStatus:
            guard response.disposition == .status else { throw NativeRuntimeError.invalidMessage }
        case .installCancel:
            guard response.disposition == .cancelAccepted else { throw NativeRuntimeError.invalidMessage }
        }
        guard observation.expectedSavedConfigurationDigest == request.expectedSavedConfigurationDigest else {
            throw NativeRuntimeError.invalidMessage
        }
        try validate(observation)
    }
}
