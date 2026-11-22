import Foundation

enum NativeRuntimeStartCodec {
    static let requestSchema = "bridgevm.app-runtime-start-request.v2"
    static let responseSchema = "bridgevm.app-runtime-start.v2"
    static let scope = "app-owned-runtime-start"
}
