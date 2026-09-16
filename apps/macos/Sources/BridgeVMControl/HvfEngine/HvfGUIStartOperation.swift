import Foundation

/// GUI admission binds the accepted editor values, not a CLI saved-config digest.
@MainActor
final class HvfGUIStartOperation {
    let id = UUID()
    let configuration: HvfEngineConfig
    let effectAdmission = HvfRuntimeEffectAdmission()
    private(set) var workerPending = true
    private(set) var outcome: HvfRuntimeStartOutcome?
    private(set) var invalidationReason: String?

    init(configuration: HvfEngineConfig) { self.configuration = configuration }

    func invalidate(_ reason: String) {
        guard workerPending else { return }
        if invalidationReason == nil { invalidationReason = reason }
        effectAdmission.invalidate()
    }

    func finish(_ outcome: HvfRuntimeStartOutcome) {
        self.outcome = outcome
        workerPending = false
        effectAdmission.invalidate()
    }
}

@MainActor
enum HvfGUIStartAdmission {
    case accepted(HvfGUIStartOperation)
    case refused(String)
}
