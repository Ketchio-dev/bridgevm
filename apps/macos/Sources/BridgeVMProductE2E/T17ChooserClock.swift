import Foundation

struct T17ChooserClock {
    let start: TimeInterval
    let deadline: TimeInterval

    init(timeout: TimeInterval, now: () -> TimeInterval) throws {
        start = now()
        deadline = start + timeout
        guard timeout.isFinite, timeout > 0, start.isFinite, deadline.isFinite else {
            throw T17FileChooser.failure("invalid path or timeout")
        }
    }
}
