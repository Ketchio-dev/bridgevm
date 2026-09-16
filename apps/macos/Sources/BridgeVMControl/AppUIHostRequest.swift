#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
struct AppUIHostRequest {
    enum Transport: String, Equatable { case arguments, environment }
    static let modeKey = "BRIDGEVM_APP_UI_HOST_MODE"
    static let outputKey = "BRIDGEVM_APP_UI_HOST_OUTPUT"
    let output: URL
    let transport: Transport
    let argumentCount: Int

    static func resolve(arguments: [String], environment: [String: String]) throws -> Self {
        let mode = environment[modeKey]
        let path = environment[outputKey]
        if mode != nil || path != nil {
            guard arguments.isEmpty else {
                throw AppUIHostError.refused("Diagnostic request cannot mix arguments and environment")
            }
            guard mode == "1", let path else {
                throw AppUIHostError.refused("Diagnostic environment requires mode 1 and an output directory")
            }
            let output = try AppUIHost.outputDirectory(arguments: ["--app-ui-host", "--output", path])
            return Self(output: output, transport: .environment, argumentCount: arguments.count)
        }
        let output = try AppUIHost.outputDirectory(arguments: arguments)
        return Self(output: output, transport: .arguments, argumentCount: arguments.count)
    }
}
#endif
