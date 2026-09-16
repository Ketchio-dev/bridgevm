import Foundation

struct HvfRuntimeLaunchFailure: Error {
    let stage: HvfRuntimeStartFailureStage
    let detail: String
}

enum HvfRuntimeLegacyLaunch {
    struct Started { let process: Process; let keyDeliveryFailure: String? }

    @MainActor
    static func start(config: HvfEngineConfig, repoRoot: URL, keyProvider: VTPMStateKeyProviding,
                      launch: (Process) throws -> Void) throws -> Started {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = config.wrapperArguments()
        process.currentDirectoryURL = repoRoot
        process.environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        let input: VTPMProcessKeyInput?
        do { input = try VTPMStateSecurity.processInput(for: config, provider: keyProvider) }
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
            return Started(process: process, keyDeliveryFailure: error.localizedDescription)
        }
        return Started(process: process, keyDeliveryFailure: nil)
    }
}
