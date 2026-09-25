import Foundation

enum HvfRuntimeLegacyFallback {
    static func available(repoRoot: URL) -> Bool {
        #if DEBUG
        return FileManager.default.isExecutableFile(atPath:
            repoRoot.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh").path)
        #else
        return false
        #endif
    }

    static func spawn(config: HvfEngineConfig, repoRoot: URL, key: Data?,
                      launch: (Process) throws -> Void) throws -> HvfRuntimeLegacyLaunch.Started {
        #if DEBUG
        return try HvfRuntimeLegacySpawn.start(config: config, repoRoot: repoRoot,
            key: key, launch: launch)
        #else
        throw HvfRuntimeLaunchFailure(stage: .helper, detail: "The packaged runner is unavailable")
        #endif
    }

    @MainActor
    static func start(config: HvfEngineConfig, repoRoot: URL, keyProvider: VTPMStateKeyProviding,
                      launch: (Process) throws -> Void) throws -> HvfRuntimeLegacyLaunch.Started {
        #if DEBUG
        return try HvfRuntimeLegacyLaunch.start(config: config, repoRoot: repoRoot,
            keyProvider: keyProvider, launch: launch)
        #else
        throw HvfRuntimeLaunchFailure(stage: .helper, detail: "The packaged runner is unavailable")
        #endif
    }
}
