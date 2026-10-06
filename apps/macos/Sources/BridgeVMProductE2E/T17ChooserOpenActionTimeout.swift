import ApplicationServices
import Foundation

extension T17ChooserOpenAction {
    static func perform(identifier: String, timeout: TimeInterval,
                        enabled: () throws -> Bool?, press: () -> AXError,
                        now: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
                        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) }) throws -> AXError {
        try perform(identifier: identifier, deadline: T17ChooserAdmission.deadline(timeout: timeout, now: now),
                    enabled: enabled, press: press, now: now, pause: pause)
    }
}
