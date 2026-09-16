import Foundation

/// Retains a noninterruptible metadata/recovery operation off the main actor.
/// Cancellation is acknowledged by the owning session at actual effect boundaries.
struct HvfWindowsInstallRecoveryWorker: Sendable {
    typealias Inspector = @Sendable (HvfWindowsInstallPlan) throws -> HvfWindowsInstallRecovery.Decision
    typealias Recoverer = @Sendable (HvfWindowsInstallPlan, HvfWindowsInstallRecovery.Ticket) throws -> VMConfig
    let inspect: Inspector
    let recover: Recoverer

    init(inspect: @escaping Inspector = { try HvfWindowsInstallRecovery.inspect(plan: $0) },
         recover: @escaping Recoverer = { try HvfWindowsInstallRecovery.recover(plan: $0, ticket: $1) }) {
        self.inspect = inspect
        self.recover = recover
    }

    func decision(for plan: HvfWindowsInstallPlan) async throws -> HvfWindowsInstallRecovery.Decision {
        try await Task.detached(priority: .userInitiated) { try inspect(plan) }.value
    }

    func resume(plan: HvfWindowsInstallPlan, ticket: HvfWindowsInstallRecovery.Ticket) async throws -> VMConfig {
        try await Task.detached(priority: .userInitiated) { try recover(plan, ticket) }.value
    }
}
