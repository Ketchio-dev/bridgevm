import Foundation

@MainActor
enum NativeRuntimeAppObservation {
    static func response(_ request: NativeRuntimeRequest, library: NativeRuntimeLibraryIdentity,
                         appInstanceID: String, retainedModel: LibraryModel?) throws -> NativeRuntimeResponse {
        try Task.checkCancellation()
        try NativeRuntimeCodec.validate(request)
        guard request.library == library, NativeLibraryReader.isCanonicalID(request.vmID),
              let retainedModel else { throw NativeRuntimeError.snapshotUnavailable }
        let sessions = try retainedModel.runtimeObservations(slug: request.vmID,
            requestedConfigurationIdentity: request.savedConfiguration.digest)
        return NativeRuntimeResponse(schema: NativeRuntimeCodec.responseSchema, requestID: request.requestID,
            library: library, vmID: request.vmID, appInstanceID: appInstanceID,
            observedAt: Date().timeIntervalSince1970, scope: NativeRuntimeCodec.scope, sessions: sessions)
    }
}
