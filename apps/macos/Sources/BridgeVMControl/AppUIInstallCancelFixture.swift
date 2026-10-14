#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
final class AppUIInstallCancelFixture {
    let root: URL
    let config: VMConfig
    let counters = AppUIInstallCancelCounters()
    let files: [AppUIInstallCancelFile]
    lazy var gate = AppUIInstallCancelGate(counters: counters)
    private(set) var session: HvfWindowsInstallSession?
    private(set) var preparation: HvfWindowsInstallPreparation?
    var repo: URL { root.appendingPathComponent("pending/bundle/repo") }

    lazy var library: LibraryModel = {
        let counts = counters
        let library = LibraryModel(rootURL: root, migrateLegacy: false,
            installSessionFactory: { [unowned self] plan in self.makeSession(plan) },
            runtimeSessionFactory: { _ in
                counts.increment("runtime_creations")
                fatalError("Synthetic install UI refused a runtime session")
            }, installPreparation: .init(repoRoot: repo, builder: { input in
                counts.increment("plan_builds")
                return input.makePlan()
            }), actionScheduler: { _ in counts.increment("file_jobs") }, startsModelsAutomatically: false,
            modelFactory: { config in
                counts.increment("model_creations")
                return ControlModel(config: config, backend: AppUIInstallCancelBackend(counts), startsAutomatically: false)
            })
        library.proMode = false
        library.selectedID = config.slug
        preparation = library.windowsInstallPreparation(for: config)
        return library
    }()

    init(root: URL) throws {
        self.root = root
        config = VMConfig(id: "pending", name: "합성 설치 UI", displayName: "합성 설치 UI", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("pending/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "owned-ui-fixture", displayWidth: 1280, displayHeight: 720,
            installPending: true, memMiB: 4096, cpuCount: 2)
        let helpers = root.appendingPathComponent("pending/bundle/repo/helpers")
        try FileManager.default.createDirectory(at: helpers, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        for name in ["wimlib-imagex", "bridgevm-catalog-verify"] {
            let file = helpers.appendingPathComponent(name)
            try Data("owned diagnostic marker; never executed\n".utf8).write(to: file, options: .withoutOverwriting)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.path)
        }
        let repo = root.appendingPathComponent("pending/bundle/repo")
        guard HvfWindowsWimlib.resolve(repoRoot: repo) == helpers.appendingPathComponent("wimlib-imagex").path,
              HvfWindowsCatalogVerifier.resolve(repoRoot: repo) == helpers.appendingPathComponent("bridgevm-catalog-verify").path,
              VMLibrary.save(config, rootURL: root) else { throw AppUIDriverFailure.invalidFile }
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        guard request.save(bundlePath: config.bundlePath) else { throw AppUIDriverFailure.invalidFile }
        files = try AppUIInstallCancelFiles.snapshot(root)
    }

    private func makeSession(_ plan: HvfWindowsInstallPlan) -> HvfWindowsInstallSession {
        counters.increment("install_creations")
        let counts = counters, validator = gate
        let execution = HvfWindowsInstallExecution(process: AppUIInstallCancelProcess(counts),
            verifySource: { _ in counts.increment("source_verifications"); return false },
            prepareMedia: { _ in counts.increment("media_preparations"); throw AppUIDriverFailure.internalFailure },
            finalize: { _ in counts.increment("finalizations"); throw AppUIDriverFailure.internalFailure })
        let result = HvfWindowsInstallSession(plan: plan, validate: { validator.validate($0) },
            schedule: { _ in counts.increment("pipeline_jobs") }, execution: execution)
        session = result
        return result
    }

    func prepared() async throws -> HvfWindowsInstallSession {
        _ = library
        guard let value = preparation else { throw AppUIDriverFailure.invalidSession }
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while case .preparing = value.state {
            guard ProcessInfo.processInfo.systemUptime < deadline else { throw AppUIDriverFailure.deadlineExceeded }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        await value.preparationTask?.value
        guard case let .ready(current) = value.state, current === session else { throw AppUIDriverFailure.invalidSession }
        let completed = current.onCompleted, counts = counters
        current.onCompleted = { counts.increment("completions"); completed?() }
        return current
    }

    func preserved() throws -> Bool { try AppUIInstallCancelFiles.snapshot(root) == files }

    func settle() async -> Bool {
        session?.cancel()
        gate.release()
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while ProcessInfo.processInfo.systemUptime < deadline {
            let counts = counters.snapshot
            let preparing: Bool
            if let preparation, case .preparing = preparation.state { preparing = true } else { preparing = false }
            if counts["validation_calls"] == counts["validation_finished"], session?.isRunning != true, !preparing {
                await preparation?.preparationTask?.value
                return true
            }
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        return false
    }
}
#endif
