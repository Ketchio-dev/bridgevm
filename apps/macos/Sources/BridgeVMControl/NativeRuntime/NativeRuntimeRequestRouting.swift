import Foundation
extension NativeRuntimeRequestRouter {
    private struct Schema: Decodable { let schema: String }

    /// Dispatches on the explicit `schema` field, so tolerant decoding cannot route a request to another handler.
    /// Each request is checked once here (size, schema, library identity); handlers trust it afterwards.
    func reply(to bytes: Data, context: NativeRuntimeRequestContext) async throws -> Data {
        try context.validateAdmission()
        let limit = NativeRuntimeCodec.maximumRequestBytes
        switch try NativeRuntimeCodec.decode(Schema.self, from: bytes, limit: limit).schema {
        case NativeRuntimeCodec.requestSchema:
            let request = try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: bytes, limit: limit)
            try NativeRuntimeCodec.validate(request)
            guard request.library == library else { throw NativeRuntimeError.libraryChanged }
            let response = try await status(request)
            try NativeRuntimeCodec.validate(response, for: request)
            return try NativeRuntimeCodec.encode(response)
        case NativeRuntimeControlCodec.requestSchema:
            let request = try NativeRuntimeCodec.decode(NativeRuntimeControlRequest.self, from: bytes, limit: limit)
            try NativeRuntimeControlCodec.validate(request)
            guard request.library == library else { throw NativeRuntimeError.libraryChanged }
            guard let control else { throw NativeRuntimeError.invalidMessage }
            return try NativeRuntimeCodec.encode(try await control(request, context))
        case NativeInstallControlCodec.requestSchema:
            return try await installReply(bytes, context: context)
        case NativeRuntimeStartCodec.requestSchema:
            let request = try NativeRuntimeCodec.decode(NativeRuntimeStartRequest.self, from: bytes, limit: limit)
            guard request.library == library else { throw NativeRuntimeError.libraryChanged }
            guard let start else { throw NativeRuntimeError.invalidMessage }
            return try NativeRuntimeCodec.encode(try await start(request, context))
        default:
            throw NativeRuntimeError.invalidMessage
        }
    }
}
