import Foundation
import Darwin

@main
enum LaunchAppUIHost {
    @MainActor static func main() {
        umask(0o077)
        let startedAt = ProcessInfo.processInfo.systemUptime
        let arguments = Array(CommandLine.arguments.dropFirst())
        do {
            #if BRIDGEVM_APP_UI_DRIVER
            if arguments.first == "--app-ui-ax-driver" { exit(AppUIDriver.run(arguments: arguments)) }
            if arguments.first == "--app-ui-supervisor-v2" {
                let paths = try AppUIHostV2Paths(arguments: arguments)
                exit(AppUIHostV2Supervisor(paths: paths, startedAt: startedAt).run())
            }
            #endif
            let paths = try AppUIHostLaunchPaths(arguments: arguments)
            exit(AppUIHostLauncher(paths: paths, startedAt: startedAt).run())
        } catch {
            FileHandle.standardError.write(Data("Native app launcher refused: \(error)\n".utf8))
            exit(2)
        }
    }
}
