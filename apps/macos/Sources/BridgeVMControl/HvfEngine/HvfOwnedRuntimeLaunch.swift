import Foundation

enum HvfOwnedRuntimeLaunch {
    @MainActor
    static func start(config: HvfEngineConfig, repoRoot: URL, runner: URL, manifest: Data,
                      keyProvider: VTPMStateKeyProviding, launch: (Process) throws -> Void)
        throws -> (controller: HvfOwnedRunController, helloFrame: Data) {
        let spawned = try VTPMRuntimeKey.withKey(for: config, provider: keyProvider) { key in
            try spawn(config: config, repoRoot: repoRoot, runner: runner, manifest: manifest, key: key, launch: launch)
        }
        return adopt(spawned)
    }
}
