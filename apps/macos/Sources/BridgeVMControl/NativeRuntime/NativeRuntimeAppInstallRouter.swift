import Foundation

@MainActor
final class NativeRuntimeAppInstallRouter {
    private let appInstanceID: String
    private let library: NativeRuntimeLibraryIdentity
    private let validateOwner: () throws -> Void
    private let retainedModel: () -> LibraryModel?

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity,
         validateOwner: @escaping () throws -> Void, retainedModel: @escaping () -> LibraryModel?) {
        self.appInstanceID = appInstanceID; self.library = library
        self.validateOwner = validateOwner; self.retainedModel = retainedModel
    }

    func handle(_ request: NativeInstallControlRequest,
                context: NativeRuntimeRequestContext) throws -> NativeInstallControlResponse {
        try NativeInstallControlCodec.validate(request)
        guard NativeLibraryReader.isCanonicalID(request.vmID) else { throw NativeRuntimeError.invalidMessage }
        try context.validateAdmission(); try validateOwner(); try context.validateAdmission()
        guard request.appInstanceID == appInstanceID else { return reply(request, refusal: .ownerChanged) }
        guard let model = retainedModel() else { return reply(request, refusal: .modelUnavailable) }
        do {
            let result = try model.requestOwnedInstall(request, validateOwner: validateOwner)
            return response(request, disposition: result.disposition,
                observation: try result.operation.observation(), refusal: nil)
        } catch let refusal as NativeInstallControlRefusal {
            return reply(request, refusal: refusal)
        }
    }

    private func reply(_ request: NativeInstallControlRequest,
                       refusal: NativeInstallControlRefusal) -> NativeInstallControlResponse {
        response(request, disposition: .refused, observation: nil, refusal: refusal)
    }

    private func response(_ request: NativeInstallControlRequest,
        disposition: NativeInstallControlResponse.Disposition, observation: NativeInstallObservation?,
        refusal: NativeInstallControlRefusal?
    ) -> NativeInstallControlResponse {
        .init(schema: NativeInstallControlCodec.responseSchema, scope: NativeInstallControlCodec.scope,
            requestID: request.requestID, library: library, vmID: request.vmID,
            appInstanceID: appInstanceID, expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest,
            requestedOperationID: request.operationID, disposition: disposition,
            observation: observation, refusal: refusal)
    }
}
