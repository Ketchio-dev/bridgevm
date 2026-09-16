import Foundation

enum HvfGUIStartWorker {
    private struct AdmissionChanged: Error {}
    private static let changed = "시작 준비 중 VM 설정 또는 작업 소유권이 변경되었습니다."

    static func run(_ input: HvfGUIStartExecution.Input) async -> HvfGUIStartWorkerOutput {
        var key: Data?
        var diagnostics: [String] = []
        defer { let count = key?.count ?? 0; key?.resetBytes(in: 0..<count) }
        do {
            try await checkAdmission(input)
            let existing = input.processIsRunning(input.config.targetDiskPath)
            try await checkAdmission(input)
            if existing {
                return .init(result: input.policy == .attachOrStart ? .observedAttachment
                    : .refused("An existing runtime cannot be adopted by a require-new launch"))
            }
            if let failure = HvfRuntimeLaunchReadiness.failure(config: input.config,
                repoRoot: input.repoRoot, diagnostic: { diagnostics.append($0) }),
               case let .failed(stage, detail) = failure {
                return .init(result: .failed(stage, detail), diagnostics: diagnostics)
            }
            try await checkAdmission(input)
            let manifest: Data
            do { manifest = try HvfRuntimePreparation.prepare(config: input.config) }
            catch { return .init(result: .failed(.preparation, error.localizedDescription)) }
            let runner = input.repoRoot.appendingPathComponent("target/release/hvf-runner")
            let typed = FileManager.default.isExecutableFile(atPath: runner.path)
            guard typed || FileManager.default.isExecutableFile(atPath:
                input.repoRoot.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh").path) else {
                return .init(result: .failed(.helper, "The installed-boot wrapper is unavailable"))
            }
            try await checkAdmission(input)
            do { key = try VTPMRuntimeKey.withKey(for: input.config, provider: input.keyProvider) { $0 } }
            catch { return .init(result: .failed(.keyAccess, error.localizedDescription)) }
            try await checkAdmission(input)
            let launch: (Process) throws -> Void = { process in
                try checkEffect(input)
                try input.launch(process)
            }
            if typed {
                let spawn = try HvfOwnedRuntimeLaunch.spawn(config: input.config, repoRoot: input.repoRoot,
                    runner: runner, manifest: manifest, key: key, launch: launch)
                return .init(result: .owned(spawn))
            }
            let spawn = try HvfRuntimeLegacySpawn.start(config: input.config, repoRoot: input.repoRoot,
                key: key, launch: launch)
            return .init(result: .legacy(spawn))
        } catch is AdmissionChanged {
            return .init(result: .refused(changed))
        } catch let failure as HvfRuntimeLaunchFailure {
            return .init(result: .failed(failure.stage, failure.detail))
        } catch {
            return .init(result: .failed(.processLaunch, error.localizedDescription))
        }
    }

    private static func checkAdmission(_ input: HvfGUIStartExecution.Input) async throws {
        guard await input.permit() else { throw AdmissionChanged() }
        try checkEffect(input)
    }

    private static func checkEffect(_ input: HvfGUIStartExecution.Input) throws {
        guard input.effectAdmission.isValid else { throw AdmissionChanged() }
    }
}
