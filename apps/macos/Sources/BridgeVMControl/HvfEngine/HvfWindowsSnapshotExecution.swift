import Foundation

extension HvfEngineSession {
    /// Creates or restores the stopped VM's powered-off snapshot under a runtime-mutation
    /// reservation, rechecked before every file and helper effect. A failure's technical detail goes
    /// to the event feed; the card shows HvfWindowsSnapshotModel.message for it.
    @discardableResult
    func runSnapshot(_ operation: HvfWindowsSnapshotCommand.Operation, configuration: HvfEngineConfig,
                     target: URL? = nil, name: String? = nil) async throws -> String? {
        do {
            return try await snapshotUnderReservation(operation, configuration: configuration, target: target, name: name)
        } catch {
            let detail = error as NSError
            append(.unknown("powered-off snapshot \(operation) failed (\(detail.domain) \(detail.code)): "
                + detail.localizedDescription))
            throw error
        }
    }

    private func snapshotUnderReservation(_ operation: HvfWindowsSnapshotCommand.Operation,
                                          configuration: HvfEngineConfig, target: URL?, name: String?) async throws -> String? {
        typealias Command = HvfWindowsSnapshotCommand
        guard configuration == config else {
            throw Command.failure("settings edited on this page are not saved yet", .settingsUnsaved)
        }
        guard let reservation = reserveRuntimeMutation(kind: .snapshot, configuration: configuration) else {
            throw Command.notStopped()
        }
        defer { finishRuntimeMutation(reservation) }
        guard validateRuntimeMutation(reservation) else { throw Command.notStopped() }
        let plan = try Command.plan(config: reservation.configuration, repoRoot: repoRoot, operation: operation,
                                    target: target, name: name)
        return try await Command.run(operation, plan: plan) {
            guard await self.validateRuntimeMutation(reservation) else { throw Command.notStopped() }
            try reservation.checkEffect()
        }
    }
}
