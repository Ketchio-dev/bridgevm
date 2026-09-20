import Foundation

@MainActor
protocol HvfWindowsInstallProcessRunning: AnyObject {
    func reset()
    func cancel()
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
        progressLog: URL?, output: @escaping (String) -> Void) async -> Bool
}

/// Explicit operations keep the session's admission and cancellation decisions
/// testable without launching an installer or preparing real guest media.
@MainActor
struct HvfWindowsInstallExecution {
    let process: any HvfWindowsInstallProcessRunning
    let verifySource: (HvfWindowsInstallPlan) async -> Bool
    let prepareMedia: (HvfWindowsInstallPlan) throws -> Void
    let finalize: (HvfWindowsInstallPlan) async throws -> Void

    init(process: (any HvfWindowsInstallProcessRunning)? = nil,
        verifySource: ((HvfWindowsInstallPlan) async -> Bool)? = nil,
        prepareMedia: ((HvfWindowsInstallPlan) throws -> Void)? = nil,
        finalize: ((HvfWindowsInstallPlan) async throws -> Void)? = nil) {
        self.process = process ?? HvfWindowsInstallProcess()
        self.verifySource = verifySource ?? { await $0.sourceImageCacheIsVerified() }
        self.prepareMedia = prepareMedia ?? { plan in
            try HvfWindowsBootSeed.writeBundledSeed(to: plan.tmpVarsPath)
            try? FileManager.default.createDirectory(atPath: plan.tmpEvidenceDir, withIntermediateDirectories: true)
        }
        let worker = HvfWindowsInstallFinalizationWorker(); self.finalize = finalize ?? { try await worker.run($0) }
    }
}
