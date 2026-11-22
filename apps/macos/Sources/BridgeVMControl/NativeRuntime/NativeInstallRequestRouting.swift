import Foundation

extension NativeRuntimeRequestRouter {
    func installReply(_ bytes: Data, context: NativeRuntimeRequestContext) async throws -> Data {
        let request = try NativeRuntimeCodec.decode(NativeInstallControlRequest.self,
            from: bytes, limit: NativeRuntimeCodec.maximumRequestBytes)
        try NativeInstallControlCodec.validate(request)
        guard request.library == library else { throw NativeRuntimeError.libraryChanged }
        guard let install else { throw NativeRuntimeError.invalidMessage }
        return try NativeRuntimeCodec.encode(try await install(request, context))
    }
}
