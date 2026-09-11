import Foundation

/// Throttle process lookups for sessions attached without an owned Process.
enum HvfAttachedLivenessSchedule {
    static let interval: TimeInterval = 1

    static func isDue(now: Date, next: Date) -> Bool { now >= next }

    static func next(after now: Date) -> Date { now.addingTimeInterval(interval) }
}
