import Foundation

enum HvfGUIStartWorker {
    private struct AdmissionChanged: Error {}
    private static let changed = "The VM settings or the operation in progress changed during startup. Check the VM’s status and try again."

    static func run(_ input: HvfGUIStartExecution.Input) async -> HvfGUIStartWorkerOutput {
        var key: Data?
        var diagnostics: [String] = []
        defer { let count = key?.count ?? 0; key?.resetBytes(in: 0..<count) }
        do {
            try await checkAdmission(input)
            if input.retryAfterRunnerDeath {
                do {
                    let lease = try HvfMediaLeaseSession(
                        executable: input.repoRoot.appendingPathComponent("target/release/examples/snapshot_pair_cli"),
                        disk: input.config.targetDiskPath, vars: input.config.uefiVarsPath)
                    try lease.finish()
                } catch {
                    diagnostics.append("Runner-death retry media lease: \(error.localizedDescription)")
                    return .init(result: .refused("The previous Windows session is still closing its files. Try starting it again in a moment."), diagnostics: diagnostics)
                }
            }
            let existing = input.retryAfterRunnerDeath ? false : input.processIsRunning(input.config.targetDiskPath)
            try await checkAdmission(input)
            if existing {
                return .init(result: input.policy == .attachOrStart ? .observedAttachment
                    : .refused("Windows is already using this VM’s disk. Open the running VM."))
            }
            if Shell.isDiskHelperRunning(matching: input.config.targetDiskPath) {
                return .init(result: .refused(VMDiskHelper.busy), diagnostics: diagnostics)
            }
            let config = input.config.preparingSharedFolder { diagnostics.append($0) }
            if let failure = HvfRuntimeLaunchReadiness.failure(config: config,
                repoRoot: input.repoRoot, diagnostic: { diagnostics.append($0) }),
               case let .failed(stage, detail) = failure {
                return .init(result: .failed(stage, detail), diagnostics: diagnostics)
            }
            try await checkAdmission(input)
            let manifest: Data
            do { manifest = try HvfRuntimePreparation.prepare(config: config) }
            catch { return .init(result: .failed(.preparation, error.localizedDescription)) }
            let runner = input.repoRoot.appendingPathComponent("target/release/hvf-runner")
            let typed = FileManager.default.isExecutableFile(atPath: runner.path)
            guard typed || HvfRuntimeLegacyFallback.available(repoRoot: input.repoRoot) else {
                return .init(result: .failed(.helper, "The installed-boot wrapper is unavailable"))
            }
            try await checkAdmission(input)
            do { key = try VTPMRuntimeKey.withKey(for: config, provider: input.keyProvider) { $0 } }
            catch {
                let stage: HvfRuntimeStartFailureStage
                switch error as? VTPMStateSecurityError {
                case .keychainRead(-25308), .keychainRead(-25293), .keychainRead(-128),
                     .keychainWrite(-25308), .keychainWrite(-25293), .keychainWrite(-128):
                    stage = .keyAccessDenied
                case .missingKeyForExistingState: stage = .keyStateMissing
                case .invalidVMIdentifier, .invalidKeyLength: stage = .keyStateInvalid
                case .randomGeneration: stage = .keyGeneration
                default: stage = .keyAccess
                }
                return .init(result: .failed(stage, error.localizedDescription))
            }
            try await checkAdmission(input)
            let launch: (Process) throws -> Void = { process in
                try checkEffect(input)
                try input.launch(process)
            }
            if typed {
                let spawn = try HvfOwnedRuntimeLaunch.spawn(config: config, repoRoot: input.repoRoot,
                    runner: runner, manifest: manifest, key: key, launch: launch)
                return .init(result: .owned(spawn), diagnostics: diagnostics)
            }
            let spawn = try HvfRuntimeLegacyFallback.spawn(config: config, repoRoot: input.repoRoot,
                key: key, launch: launch)
            return .init(result: .legacy(spawn), diagnostics: diagnostics)
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
