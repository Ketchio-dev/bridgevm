import SwiftUI

extension HvfWindowsSnapshotCard {
    func perform(_ operation: HvfWindowsSnapshotCommand.Operation) {
        guard !busy, let reservation = session.reserveRuntimeMutation(kind: .snapshot, configuration: config) else { return }
        busy = true
        status = operation == .create ? "스냅샷 생성 중…" : "스냅샷 복원 중…"
        do {
            guard session.validateRuntimeMutation(reservation) else {
                throw NSError(domain: "BridgeVM.RuntimeMutation", code: 1)
            }
            let plan = try HvfWindowsSnapshotCommand.plan(config: reservation.configuration, repoRoot: repoRoot, operation: operation)
            Task {
                do {
                    guard session.validateRuntimeMutation(reservation) else {
                        throw NSError(domain: "BridgeVM.RuntimeMutation", code: 1)
                    }
                    _ = try await HvfWindowsSnapshotCommand.run(operation, plan: plan) {
                        guard await session.validateRuntimeMutation(reservation) else {
                            throw NSError(domain: "BridgeVM.RuntimeMutation", code: 1)
                        }
                        try reservation.checkEffect()
                    }
                    status = operation == .create ? "스냅샷 생성 완료" : "스냅샷 복원 완료"
                } catch { status = "스냅샷 실패: \(error.localizedDescription)" }
                session.finishRuntimeMutation(reservation)
                busy = false
            }
        } catch {
            status = "스냅샷 실패: \(error.localizedDescription)"
            session.finishRuntimeMutation(reservation)
            busy = false
        }
    }
}
