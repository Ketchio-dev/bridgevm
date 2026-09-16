import Foundation

struct NativeRuntimeRequestRouter: Sendable {
    typealias ControlHandler = @Sendable (NativeRuntimeControlRequest, NativeRuntimeRequestContext) async throws -> NativeRuntimeControlResponse
    let library: NativeRuntimeLibraryIdentity
    let status: NativeRuntimeServer.Handler
    let control: ControlHandler?

    func reply(to bytes: Data, context: NativeRuntimeRequestContext) async throws -> Data {
        try context.validateAdmission()
        if let request = try? NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: bytes,
                                                        limit: NativeRuntimeCodec.maximumRequestBytes) {
            try NativeRuntimeCodec.validate(request)
            guard request.library == library else { throw NativeRuntimeError.libraryChanged }
            let response = try await status(request)
            try NativeRuntimeCodec.validate(response, for: request)
            return try NativeRuntimeCodec.encode(response)
        }
        let request = try NativeRuntimeCodec.decode(NativeRuntimeControlRequest.self, from: bytes,
                                                    limit: NativeRuntimeCodec.maximumRequestBytes)
        try NativeRuntimeControlCodec.validate(request)
        guard request.library == library else { throw NativeRuntimeError.libraryChanged }
        guard let control else { throw NativeRuntimeError.invalidMessage }
        let response = try await control(request, context)
        try NativeRuntimeControlCodec.validate(response, for: request)
        return try NativeRuntimeCodec.encode(response)
    }
}
