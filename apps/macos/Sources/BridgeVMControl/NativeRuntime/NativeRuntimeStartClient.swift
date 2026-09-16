import Foundation

enum NativeRuntimeStartClient {
    static func query(rootURL: URL, request: NativeRuntimeStartRequest) throws -> NativeRuntimeStartResponse {
        try NativeRuntimeStartCodec.validate(request)
        return try NativeRuntimeClientExchange.perform(rootURL: rootURL, identity: request.library,
                                                       bytes: NativeRuntimeCodec.encode(request)) { data in
            let response = try NativeRuntimeCodec.decode(NativeRuntimeStartResponse.self, from: data,
                                                         limit: NativeRuntimeCodec.maximumResponseBytes)
            try NativeRuntimeStartCodec.validate(response, for: request)
            return response
        }
    }
}
