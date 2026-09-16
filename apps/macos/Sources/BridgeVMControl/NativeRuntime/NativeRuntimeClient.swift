import Darwin
import Foundation

enum NativeRuntimeClient {
    static func query(rootURL: URL, request: NativeRuntimeRequest) throws -> NativeRuntimeResponse {
        try NativeRuntimeCodec.validate(request)
        return try NativeRuntimeClientExchange.perform(rootURL: rootURL, identity: request.library,
                                                       bytes: NativeRuntimeCodec.encode(request)) { data in
            let response = try NativeRuntimeCodec.decode(NativeRuntimeResponse.self, from: data,
                                                         limit: NativeRuntimeCodec.maximumResponseBytes)
            try NativeRuntimeCodec.validate(response, for: request)
            return response
        }
    }
}
