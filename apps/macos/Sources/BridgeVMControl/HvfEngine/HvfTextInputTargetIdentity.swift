import Foundation

struct HvfTextInputTargetIdentity: Equatable {
    let owner: ObjectIdentifier
    let generation: UUID
    let binding: [String]
}

extension HvfSessionInputDriver {
    func invalidatesTextInputTarget(previous: HvfNegotiatedInputStream.State, lines: [String]) -> Bool {
        (previous == .ready && router.state != .ready) || lines.contains(where: {
            HvfGuestResetRecord.matches($0) || $0.hasPrefix("BVAGENT READY") ||
                $0.hasPrefix("BVAGENT re-READY") || $0.hasPrefix("BVAGENT SERVICE start")
        })
    }

    func reportPollUpdate(_ update: HvfAcknowledgedInputStream.Update?, previous: HvfNegotiatedInputStream.State? = nil) {
        if case let .cancelled(reason, discarded) = update, discarded > 0 {
            onDiagnostic?("ordered input cancelled: \(reason), discarded=\(discarded)")
        }
        if let previous, previous != .ready, router.state == .ready {
            onDiagnostic?("ordered input active: receipts confirm insertion, not application consumption")
        }
    }
}
