import Foundation

/// Runs the durable, noninterruptible installation commit without occupying
/// the app's main actor. Cancellation belongs to the owning session and is
/// acknowledged only before commit begins.
struct HvfWindowsInstallFinalizationWorker: Sendable {
    typealias Finalizer = @Sendable (HvfWindowsInstallPlan) throws -> Void
    let finalize: Finalizer

    init(finalize: @escaping Finalizer = {
        try HvfWindowsInstallFinalization.finalize(plan: $0)
    }) {
        self.finalize = finalize
    }

    func run(_ plan: HvfWindowsInstallPlan) async throws {
        try await Task.detached(priority: .userInitiated) {
            try finalize(plan)
        }.value
    }
}
