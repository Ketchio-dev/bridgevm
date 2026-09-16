import Foundation

enum HvfRuntimeLegacySpawn {
    static func start(config: HvfEngineConfig, repoRoot: URL, key: Data?,
                      launch: (Process) throws -> Void) throws -> HvfRuntimeLegacyLaunch.Started {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = config.wrapperArguments()
        process.currentDirectoryURL = repoRoot
        process.environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        let input: VTPMProcessKeyInput?
        do { input = try key.map { try VTPMProcessKeyInput(key: $0) } }
        catch { throw HvfRuntimeLaunchFailure(stage: .keyAccess, detail: error.localizedDescription) }
        input?.attach(to: process)
        do { try launch(process) }
        catch {
            input?.discard()
            throw HvfRuntimeLaunchFailure(stage: .processLaunch, detail: error.localizedDescription)
        }
        do { try input?.deliverAfterLaunch() }
        catch {
            if process.isRunning { process.terminate() }
            return .init(process: process, keyDeliveryFailure: error.localizedDescription)
        }
        return .init(process: process, keyDeliveryFailure: nil)
    }
}
