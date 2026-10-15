import Foundation

extension HvfEngineSession {
    // The injected launcher must run the supplied Process or throw before spawning it.
    convenience init(config: HvfEngineConfig, repoRoot: URL,
                     processLaunch: @escaping (Process) throws -> Void,
                     processIsRunning: @escaping (String) -> Bool,
                     vtpmKeyProvider: VTPMStateKeyProviding) {
        self.init(config: config, repoRoot: repoRoot,
                  processIsRunning: processIsRunning, vtpmKeyProvider: vtpmKeyProvider)
        self.processLaunch = processLaunch
    }
}
