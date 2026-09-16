import Foundation

extension NativeRuntimeTransportConnection {
    func begin(request: NativeRuntimeRequest, handler: @escaping NativeRuntimeServer.Handler,
               finished: @escaping @Sendable () -> Void) {
        begin(work: {
            let response = try await handler(request)
            try NativeRuntimeCodec.validate(response, for: request)
            return try NativeRuntimeCodec.encode(response)
        }, finished: finished)
    }

    func response() throws -> NativeRuntimeResponse {
        try NativeRuntimeCodec.decode(NativeRuntimeResponse.self, from: responseData(),
                                      limit: NativeRuntimeCodec.maximumResponseBytes)
    }
}
