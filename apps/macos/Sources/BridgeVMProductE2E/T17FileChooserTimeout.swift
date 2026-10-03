import Foundation

extension T17FileChooser {
    static func choose(
        path: String, timeout: TimeInterval, driver: T17FileChooserDriving,
        now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
    ) throws {
        let clock = try T17ChooserClock(timeout: timeout, now: now), deadline = clock.deadline
        let timing = T17ChooserTiming(start: clock.start, deadline: deadline, now: now)
        do {
            try choose(path: path, deadline: deadline, driver: driver, timing: timing, now: timing.checkedNow, pause: pause)
        } catch let blocker as T17Blocker {
            throw timing.attributed(blocker)
        }
    }
}
