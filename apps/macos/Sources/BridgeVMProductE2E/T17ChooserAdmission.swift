import Foundation

enum T17ChooserAdmission {
    static func deadline(timeout: TimeInterval, now: () -> TimeInterval) throws -> TimeInterval {
        try T17ChooserBudget.deadline(timeout: timeout, now: now)
    }

    static func choose<Target>(
        path: String, timeout: TimeInterval,
        now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) },
        lookup: (TimeInterval) throws -> Target,
        driver: (Target, TimeInterval, @escaping () -> TimeInterval) throws -> T17FileChooserDriving
    ) throws {
        guard path.hasPrefix("/"), !path.contains("\0") else {
            throw T17FileChooser.failure("invalid path or timeout")
        }
        let clock = try T17ChooserClock(timeout: timeout, now: now), deadline = clock.deadline
        let timing = T17ChooserTiming(start: clock.start, deadline: deadline, now: now)
        do {
            let target = try timing.run("target-lookup") {
                try lookup(T17ChooserBudget.remaining(deadline: deadline, now: timing.checkedNow))
            }
            _ = try T17ChooserBudget.remaining(deadline: deadline, now: timing.checkedNow)
            try T17FileChooser.choose(path: path, deadline: deadline,
                driver: driver(target, deadline, timing.checkedNow), timing: timing, now: timing.checkedNow, pause: pause)
        } catch let blocker as T17Blocker {
            throw timing.attributed(blocker)
        }
    }
}
