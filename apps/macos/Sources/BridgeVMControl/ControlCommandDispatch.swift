import Foundation
import Darwin

enum ControlCommandDispatch {
    static func validateOrExit(arguments: [String]) {
        guard !arguments.contains("--app-ui-host") else {
            FileHandle.standardError.write(Data("This executable is not the diagnostic host.\n".utf8))
            exit(2)
        }
        if arguments.first == "--vtpm-lifecycle" {
            exit(VTPMLifecycleCommand.run(arguments: Array(arguments.dropFirst())))
        }
        if arguments.first == "--cli" {
            exit(NativeCLI.run(arguments: Array(arguments.dropFirst())))
        }
        BridgeVMControlLaunchOptions.validateOrExit(arguments: arguments)
    }
}
