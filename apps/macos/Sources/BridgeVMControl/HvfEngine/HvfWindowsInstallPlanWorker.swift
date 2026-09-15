import Foundation

struct HvfWindowsInstallPlanInput: Equatable, Sendable {
    let config: VMConfig
    let request: HvfWindowsInstallRequest
    let libraryRoot: URL
    let repoRoot: URL

    static func load(for config: VMConfig, libraryRoot: URL, repoRoot: URL) -> Self {
        let request = HvfWindowsInstallRequest.load(bundlePath: config.bundlePath)
            ?? HvfWindowsInstallRequest(isoPath: "", diskGiB: 64,
                                       injectViogpu3d: false, driverPackageDir: nil)
        return Self(config: config, request: request, libraryRoot: libraryRoot, repoRoot: repoRoot)
    }

    func makePlan() -> HvfWindowsInstallPlan {
        HvfWindowsInstallPlan(repoRoot: repoRoot, libraryRoot: libraryRoot,
            bundlePath: config.bundlePath, slug: config.slug, request: request)
    }
}

enum HvfWindowsInstallPlanWorker {
    typealias Builder = @Sendable (HvfWindowsInstallPlanInput) -> HvfWindowsInstallPlan

    static func build(_ input: HvfWindowsInstallPlanInput, using builder: @escaping Builder) async -> HvfWindowsInstallPlan {
        await Task.detached(priority: .userInitiated) { builder(input) }.value
    }
}

struct HvfWindowsInstallPreparationOptions {
    let repoRoot: URL
    let builder: HvfWindowsInstallPlanWorker.Builder

    init(repoRoot: URL = HvfEngineSession.defaultRepoRoot(),
         builder: @escaping HvfWindowsInstallPlanWorker.Builder = { $0.makePlan() }) {
        self.repoRoot = repoRoot
        self.builder = builder
    }
}
