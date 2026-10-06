import Foundation

enum T17ChooserBudget {
    static func deadline(timeout: TimeInterval, now: () -> TimeInterval) throws -> TimeInterval {
        try T17ChooserClock(timeout: timeout, now: now).deadline
    }

    static func remaining(deadline: TimeInterval, now: () -> TimeInterval) throws -> TimeInterval {
        let time = now()
        guard time.isFinite, time < deadline else {
            throw T17FileChooser.failure("stage=chooser-admission; timed out before chooser stage")
        }
        return deadline - time
    }
}
