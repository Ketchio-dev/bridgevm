import Foundation
import SwiftUI
import Darwin

#if BRIDGEVM_APP_UI_HOST && !DEBUG
#error("BRIDGEVM_APP_UI_HOST requires a separate DEBUG diagnostic build")
#endif
@main
@MainActor
enum BridgeVMControlMain {
    static func main() {
        let arguments = Array(CommandLine.arguments.dropFirst())
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        do {
            try AppUIHost.prepare(arguments: arguments)
            try AppUIHost.prepared?.checkBeforeApplication()
        }
        catch {
            FileHandle.standardError.write(Data("App UI host refused: \(error)\n".utf8))
            exit(2)
        }
        #else
        guard !arguments.contains("--app-ui-host") else {
            FileHandle.standardError.write(Data("This executable is not the diagnostic host.\n".utf8))
            exit(2)
        }
        if arguments.first == "--vtpm-lifecycle" {
            exit(VTPMLifecycleCommand.run(arguments: Array(arguments.dropFirst())))
        }
        BridgeVMControlLaunchOptions.validateOrExit(arguments: arguments)
        #endif
        BridgeVMControlApp.main()
    }
}
