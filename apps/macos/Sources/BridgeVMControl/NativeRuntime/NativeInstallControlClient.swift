import Foundation

enum NativeInstallControlClient {
    static func query(rootURL: URL,
                      request: NativeInstallControlRequest) throws -> NativeInstallControlResponse {
        try NativeInstallControlCodec.validate(request)
        return try NativeRuntimeClientExchange.perform(rootURL: rootURL, identity: request.library,
            bytes: NativeRuntimeCodec.encode(request)) { data in
            let response = try NativeRuntimeCodec.decode(NativeInstallControlResponse.self,
                from: data, limit: NativeRuntimeCodec.maximumResponseBytes)
            try NativeInstallControlCodec.validate(response, for: request)
            return response
        }
    }
}
