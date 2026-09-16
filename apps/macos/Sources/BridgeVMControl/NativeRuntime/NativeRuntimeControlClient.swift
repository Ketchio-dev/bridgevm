import Foundation

enum NativeRuntimeControlClient {
    static func query(rootURL: URL, request: NativeRuntimeControlRequest) throws -> NativeRuntimeControlResponse {
        try NativeRuntimeControlCodec.validate(request)
        return try NativeRuntimeClientExchange.perform(rootURL: rootURL, identity: request.library,
                                                       bytes: NativeRuntimeCodec.encode(request)) { data in
            let response = try NativeRuntimeCodec.decode(NativeRuntimeControlResponse.self, from: data,
                                                         limit: NativeRuntimeCodec.maximumResponseBytes)
            try NativeRuntimeControlCodec.validate(response, for: request)
            return response
        }
    }
}
