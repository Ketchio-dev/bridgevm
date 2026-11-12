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
    let prepareMedia: (HvfWindowsInstallPlan) async throws -> Void
    let finalize: (HvfWindowsInstallPlan) async throws -> Void

    init(process: (any HvfWindowsInstallProcessRunning)? = nil,
        verifySource: ((HvfWindowsInstallPlan) async -> Bool)? = nil,
        prepareMedia: ((HvfWindowsInstallPlan) async throws -> Void)? = nil,
        preparationWorker: @escaping @Sendable (HvfWindowsInstallPlan) throws -> Void = { try HvfWindowsInstallStaging.prepare($0) },
        finalize: ((HvfWindowsInstallPlan) async throws -> Void)? = nil) {
        self.process = process ?? HvfWindowsInstallProcess()
        self.verifySource = verifySource ?? { await $0.sourceImageCacheIsVerified() }
        self.prepareMedia = prepareMedia ?? { plan in
            try await Task.detached(priority: .utility) { try preparationWorker(plan) }.value
        }
        let worker = HvfWindowsInstallFinalizationWorker(); self.finalize = finalize ?? { try await worker.run($0) }
    }
}
