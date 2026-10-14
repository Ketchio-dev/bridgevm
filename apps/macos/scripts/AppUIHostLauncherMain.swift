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
            if let result = try AppUIHostV2Mode.dispatch(arguments: arguments, startedAt: startedAt) { exit(result) }
            #endif
            let paths = try AppUIHostLaunchPaths(arguments: arguments)
            exit(AppUIHostLauncher(paths: paths, startedAt: startedAt).run())
        } catch {
            FileHandle.standardError.write(Data("Native app launcher refused: \(error)\n".utf8))
            exit(2)
        }
    }
}
