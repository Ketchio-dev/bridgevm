import Foundation

enum T17ChooserAdmission {
    static func deadline(timeout: TimeInterval, now: () -> TimeInterval) throws -> TimeInterval {
        let start = now(), deadline = start + timeout
        guard timeout.isFinite, timeout > 0, start.isFinite, deadline.isFinite else {
            throw T17FileChooser.failure("invalid path or timeout")
        }
        return deadline
    }

    static func choose<Target>(
        path: String, timeout: TimeInterval,
        now: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) },
        lookup: (TimeInterval) throws -> Target,
        driver: (Target, TimeInterval) throws -> T17FileChooserDriving
    ) throws {
        guard path.hasPrefix("/"), !path.contains("\0") else {
            throw T17FileChooser.failure("invalid path or timeout")
        }
        let deadline = try deadline(timeout: timeout, now: now)
        func remaining() throws -> TimeInterval {
            let time = now()
            guard time.isFinite, time < deadline else {
                throw T17FileChooser.failure("stage=chooser-admission; timed out before chooser stage")
            }
            return deadline - time
        }
        let target = try lookup(remaining())
        _ = try remaining()
        try T17FileChooser.choose(path: path, deadline: deadline,
            driver: driver(target, deadline), now: now, pause: pause)
    }
}
