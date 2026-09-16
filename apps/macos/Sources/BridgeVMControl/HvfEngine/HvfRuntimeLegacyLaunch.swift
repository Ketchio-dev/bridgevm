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
        var key: Data?
        defer { let count = key?.count ?? 0; key?.resetBytes(in: 0..<count) }
        do { key = try VTPMRuntimeKey.withKey(for: config, provider: keyProvider) { $0 } }
        catch { throw HvfRuntimeLaunchFailure(stage: .keyAccess, detail: error.localizedDescription) }
        return try HvfRuntimeLegacySpawn.start(config: config, repoRoot: repoRoot, key: key, launch: launch)
    }
}
