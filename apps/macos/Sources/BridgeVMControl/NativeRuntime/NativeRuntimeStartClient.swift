import Foundation

enum NativeRuntimeStartClient {
    static func query(rootURL: URL, request: NativeRuntimeStartRequest) throws -> NativeRuntimeStartResponse {
        try NativeRuntimeClientExchange.perform(rootURL: rootURL, identity: request.library,
                                                bytes: NativeRuntimeCodec.encode(request)) { data in
            try NativeRuntimeCodec.decode(NativeRuntimeStartResponse.self, from: data,
                                          limit: NativeRuntimeCodec.maximumResponseBytes)
        }
    }
}
